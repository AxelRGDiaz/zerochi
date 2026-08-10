use super::{
    AddAccountError, AddAccountErrorTag, AuthorizationCallback, CallbackPort, CallbackSession,
    Cancellation, PortFuture, SensitiveString,
};
use std::{
    io::{ErrorKind, Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};
use zeroize::{Zeroize, Zeroizing};

const CALLBACK_PATH: &str = "/oauth/callback";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const IO_POLL: Duration = Duration::from_millis(25);
const CALLBACK_HTML: &str = "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\"><title>Zerochi</title></head><body><p>You can return to Zerochi.</p></body></html>";

pub(crate) struct LoopbackCallbackPort {
    timeout: Duration,
}

impl Default for LoopbackCallbackPort {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl LoopbackCallbackPort {
    #[cfg(test)]
    fn with_timeout(timeout: Duration) -> Self {
        Self { timeout }
    }
}

impl CallbackPort for LoopbackCallbackPort {
    fn bind(&self) -> Result<Box<dyn CallbackSession>, AddAccountError> {
        let listener =
            TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|_| callback_invalid())?;
        listener
            .set_nonblocking(true)
            .map_err(|_| callback_invalid())?;
        let address = listener.local_addr().map_err(|_| callback_invalid())?;
        if !matches!(address, SocketAddr::V4(address) if address.ip().is_loopback()) {
            return Err(callback_invalid());
        }

        Ok(Box::new(LoopbackCallbackSession {
            redirect_uri: format!("http://127.0.0.1:{}{CALLBACK_PATH}", address.port()),
            authority: format!("127.0.0.1:{}", address.port()),
            listener: Some(listener),
            timeout: self.timeout,
        }))
    }
}

struct LoopbackCallbackSession {
    redirect_uri: String,
    authority: String,
    listener: Option<TcpListener>,
    timeout: Duration,
}

impl CallbackSession for LoopbackCallbackSession {
    fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }

    fn receive<'a>(
        &'a mut self,
        expected_state: &'a str,
        cancellation: &'a Cancellation,
    ) -> PortFuture<'a, Result<AuthorizationCallback, AddAccountError>> {
        let listener = self.listener.take();
        let expected_state = SensitiveString::new(expected_state.to_owned());
        let expected_authority = self.authority.clone();
        let cancellation = cancellation.clone();
        let timeout = self.timeout;
        Box::pin(async move {
            let listener = listener.ok_or_else(callback_invalid)?;
            tauri::async_runtime::spawn_blocking(move || {
                receive_blocking(
                    listener,
                    expected_state,
                    expected_authority,
                    cancellation,
                    timeout,
                )
            })
            .await
            .map_err(|_| internal())?
        })
    }
}

fn receive_blocking(
    listener: TcpListener,
    expected_state: SensitiveString,
    expected_authority: String,
    cancellation: Cancellation,
    timeout: Duration,
) -> Result<AuthorizationCallback, AddAccountError> {
    let deadline = Instant::now() + timeout;
    loop {
        cancellation.ensure_active()?;
        if Instant::now() >= deadline {
            return Err(AddAccountError::from_tag(AddAccountErrorTag::Timeout));
        }

        match listener.accept() {
            Ok((mut stream, peer)) => {
                if !peer.ip().is_loopback() {
                    return Err(callback_invalid());
                }
                let result = read_callback(
                    &mut stream,
                    &expected_state,
                    &expected_authority,
                    &cancellation,
                    deadline,
                );
                write_static_html(&mut stream);
                return result;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(IO_POLL.min(deadline.saturating_duration_since(Instant::now())));
            }
            Err(_) => return Err(callback_invalid()),
        }
    }
}

fn read_callback(
    stream: &mut TcpStream,
    expected_state: &SensitiveString,
    expected_authority: &str,
    cancellation: &Cancellation,
    deadline: Instant,
) -> Result<AuthorizationCallback, AddAccountError> {
    stream
        .set_read_timeout(Some(IO_POLL))
        .map_err(|_| callback_invalid())?;
    let mut buffer = Zeroizing::new([0_u8; MAX_REQUEST_BYTES]);
    let mut used = 0;

    loop {
        cancellation.ensure_active()?;
        if Instant::now() >= deadline {
            return Err(AddAccountError::from_tag(AddAccountErrorTag::Timeout));
        }
        if used == buffer.len() {
            return Err(callback_invalid());
        }

        match stream.read(&mut buffer[used..]) {
            Ok(0) => return Err(callback_invalid()),
            Ok(count) => {
                used += count;
                if let Some(header_end) = find_header_end(&buffer[..used]) {
                    return parse_request(
                        &buffer[..header_end],
                        expected_state,
                        expected_authority,
                    );
                }
            }
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
            Err(_) => return Err(callback_invalid()),
        }
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|at| at + 4)
}

fn parse_request(
    request: &[u8],
    expected_state: &SensitiveString,
    expected_authority: &str,
) -> Result<AuthorizationCallback, AddAccountError> {
    if request.len() > MAX_REQUEST_BYTES {
        return Err(callback_invalid());
    }
    let first_line_end = request
        .windows(2)
        .position(|window| window == b"\r\n")
        .ok_or_else(callback_invalid)?;
    let first_line = &request[..first_line_end];
    let mut parts = first_line.split(|byte| *byte == b' ');
    let method = parts.next().ok_or_else(callback_invalid)?;
    let target = parts.next().ok_or_else(callback_invalid)?;
    let version = parts.next().ok_or_else(callback_invalid)?;
    if parts.next().is_some() || method != b"GET" || !matches!(version, b"HTTP/1.1" | b"HTTP/1.0") {
        return Err(callback_invalid());
    }
    if !target.starts_with(b"/") || target.starts_with(b"//") {
        return Err(callback_invalid());
    }
    validate_host_header(request, first_line_end + 2, expected_authority)?;

    let (path, query) = split_at_byte(target, b'?').ok_or_else(callback_invalid)?;
    if path != CALLBACK_PATH.as_bytes() || query.is_empty() {
        return Err(callback_invalid());
    }

    let mut seen_names: Vec<Vec<u8>> = Vec::new();
    let mut code = None;
    let mut state = None;
    let mut provider_error = None;
    for pair in query.split(|byte| *byte == b'&') {
        let (raw_name, raw_value) = split_at_byte(pair, b'=').unwrap_or((pair, b""));
        let name = percent_decode(raw_name)?;
        if seen_names.iter().any(|seen| seen == name.as_slice()) {
            return Err(callback_invalid());
        }
        seen_names.push(name.to_vec());

        match name.as_slice() {
            b"code" => code = Some(decode_sensitive(raw_value)?),
            b"state" => state = Some(decode_sensitive(raw_value)?),
            b"error" => provider_error = Some(decode_sensitive(raw_value)?),
            _ => {}
        }
    }

    let state = state.ok_or_else(callback_invalid)?;
    if !constant_time_equal(state.expose_bytes(), expected_state.expose_bytes()) {
        return Err(callback_invalid());
    }
    if code.is_some() == provider_error.is_some() {
        return Err(callback_invalid());
    }
    if let Some(error) = provider_error {
        return Err(classify_provider_error(error.expose()));
    }
    let code = code
        .filter(|code| !code.is_empty())
        .ok_or_else(callback_invalid)?;
    Ok(AuthorizationCallback { code })
}

fn validate_host_header(
    request: &[u8],
    mut cursor: usize,
    expected_authority: &str,
) -> Result<(), AddAccountError> {
    let mut host_seen = false;
    loop {
        let relative_end = request[cursor..]
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(callback_invalid)?;
        let line_end = cursor + relative_end;
        let line = &request[cursor..line_end];
        cursor = line_end + 2;
        if line.is_empty() {
            if cursor != request.len() {
                return Err(callback_invalid());
            }
            break;
        }
        if line.first().is_some_and(u8::is_ascii_whitespace) {
            return Err(callback_invalid());
        }
        let (name, raw_value) = split_at_byte(line, b':').ok_or_else(callback_invalid)?;
        if name.is_empty() || !name.iter().all(|byte| is_header_name_byte(*byte)) {
            return Err(callback_invalid());
        }
        if !raw_value
            .iter()
            .all(|byte| *byte == b'\t' || (*byte >= b' ' && *byte <= b'~'))
        {
            return Err(callback_invalid());
        }
        if name.eq_ignore_ascii_case(b"host") {
            if host_seen {
                return Err(callback_invalid());
            }
            host_seen = true;
            let value = trim_optional_whitespace(raw_value);
            if value != expected_authority.as_bytes() {
                return Err(callback_invalid());
            }
        }
    }
    if !host_seen {
        return Err(callback_invalid());
    }
    Ok(())
}

fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn trim_optional_whitespace(mut value: &[u8]) -> &[u8] {
    while value
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[1..];
    }
    while value
        .last()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
    {
        value = &value[..value.len() - 1];
    }
    value
}

fn split_at_byte(bytes: &[u8], needle: u8) -> Option<(&[u8], &[u8])> {
    let index = bytes.iter().position(|byte| *byte == needle)?;
    Some((&bytes[..index], &bytes[index + 1..]))
}

fn percent_decode(value: &[u8]) -> Result<Zeroizing<Vec<u8>>, AddAccountError> {
    let mut decoded = Zeroizing::new(Vec::with_capacity(value.len()));
    let mut index = 0;
    while index < value.len() {
        match value[index] {
            b'+' => decoded.push(b' '),
            b'%' if index + 2 < value.len() => {
                let high = hex(value[index + 1]).ok_or_else(callback_invalid)?;
                let low = hex(value[index + 2]).ok_or_else(callback_invalid)?;
                decoded.push((high << 4) | low);
                index += 2;
            }
            b'%' => return Err(callback_invalid()),
            byte if byte.is_ascii() && !byte.is_ascii_control() => decoded.push(byte),
            _ => return Err(callback_invalid()),
        }
        index += 1;
    }
    Ok(decoded)
}

fn decode_sensitive(value: &[u8]) -> Result<SensitiveString, AddAccountError> {
    let mut decoded = percent_decode(value)?;
    let string = String::from_utf8(decoded.to_vec()).map_err(|_| callback_invalid())?;
    decoded.zeroize();
    Ok(SensitiveString::new(string))
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn constant_time_equal(left: &[u8], right: &[u8]) -> bool {
    let mut difference = left.len() ^ right.len();
    let longest = left.len().max(right.len());
    for index in 0..longest {
        difference |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    difference == 0
}

fn classify_provider_error(error: &str) -> AddAccountError {
    let tag = match error {
        "access_denied" => AddAccountErrorTag::Cancelled,
        "admin_policy_enforced" | "org_internal" | "restricted_client" => {
            AddAccountErrorTag::WorkspaceRestricted
        }
        _ => AddAccountErrorTag::ProviderDenied,
    };
    AddAccountError::from_tag(tag)
}

fn write_static_html(stream: &mut TcpStream) {
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nContent-Security-Policy: default-src 'none'; style-src 'none'; img-src 'none'\r\nReferrer-Policy: no-referrer\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{CALLBACK_HTML}",
        CALLBACK_HTML.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

fn callback_invalid() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::CallbackInvalid)
}

fn internal() -> AddAccountError {
    AddAccountError::from_tag(AddAccountErrorTag::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AddAccountAttemptGuardSet;

    const STATE: &str = "fixture-state-0123456789";
    const AUTHORITY: &str = "127.0.0.1:49152";

    fn parse(target: &str) -> Result<AuthorizationCallback, AddAccountError> {
        let request = format!("GET {target} HTTP/1.1\r\nHost: {AUTHORITY}\r\n\r\n");
        parse_request(
            request.as_bytes(),
            &SensitiveString::new(STATE.into()),
            AUTHORITY,
        )
    }

    #[test]
    fn validates_the_exact_callback_and_decodes_the_code() {
        let callback = parse(&format!(
            "{CALLBACK_PATH}?code=fixture%2Dcode&state={STATE}"
        ))
        .expect("valid callback");
        assert_eq!(callback.code.expose(), "fixture-code");
    }

    #[test]
    fn rejects_method_path_state_duplicates_and_code_error_ambiguity() {
        let invalid = [
            format!("POST {CALLBACK_PATH}?code=x&state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET /other?code=x&state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=x HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=x&state=wrong HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=x&code=y&state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=x&state={STATE}&state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=x&error=nope&state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?state={STATE} HTTP/1.1\r\n\r\n"),
            format!("GET {CALLBACK_PATH}?code=&state={STATE} HTTP/1.1\r\n\r\n"),
        ];

        for request in invalid {
            let error = parse_request(
                request.as_bytes(),
                &SensitiveString::new(STATE.into()),
                AUTHORITY,
            )
            .unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::CallbackInvalid);
        }
    }

    #[test]
    fn rejects_missing_wrong_duplicate_or_malformed_host_and_absolute_targets() {
        let target = format!("{CALLBACK_PATH}?code=x&state={STATE}");
        let invalid = [
            format!("GET {target} HTTP/1.1\r\nAccept: */*\r\n\r\n"),
            format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:1\r\n\r\n"),
            format!("GET {target} HTTP/1.1\r\nHost: {AUTHORITY}\r\nhOsT: {AUTHORITY}\r\n\r\n"),
            format!("GET {target} HTTP/1.1\r\nHost : {AUTHORITY}\r\n\r\n"),
            format!("GET {target} HTTP/1.1\r\nHost: {AUTHORITY},evil.test\r\n\r\n"),
            format!("GET {target} HTTP/1.1\r\nHost: {AUTHORITY}\r\n folded\r\n\r\n"),
            format!("GET http://{AUTHORITY}{target} HTTP/1.1\r\nHost: {AUTHORITY}\r\n\r\n"),
            format!("GET //{AUTHORITY}{target} HTTP/1.1\r\nHost: {AUTHORITY}\r\n\r\n"),
        ];

        for request in invalid {
            let error = parse_request(
                request.as_bytes(),
                &SensitiveString::new(STATE.into()),
                AUTHORITY,
            )
            .unwrap_err();
            assert_eq!(error.tag(), AddAccountErrorTag::CallbackInvalid);
        }
    }

    #[test]
    fn accepts_one_case_insensitive_host_header_with_optional_whitespace() {
        let request = format!(
            "GET {CALLBACK_PATH}?code=x&state={STATE} HTTP/1.1\r\nhOsT:\t{AUTHORITY} \r\nAccept: text/html\r\n\r\n"
        );
        let callback = parse_request(
            request.as_bytes(),
            &SensitiveString::new(STATE.into()),
            AUTHORITY,
        )
        .expect("valid host");
        assert_eq!(callback.code.expose(), "x");
    }

    #[test]
    fn maps_user_denial_workspace_policy_and_unknown_provider_errors() {
        for (provider_error, expected) in [
            ("access_denied", AddAccountErrorTag::Cancelled),
            (
                "admin_policy_enforced",
                AddAccountErrorTag::WorkspaceRestricted,
            ),
            ("org_internal", AddAccountErrorTag::WorkspaceRestricted),
            ("unknown", AddAccountErrorTag::ProviderDenied),
        ] {
            let error = parse(&format!(
                "{CALLBACK_PATH}?error={provider_error}&state={STATE}"
            ))
            .unwrap_err();
            assert_eq!(error.tag(), expected);
            assert!(!format!("{error:?}").contains(provider_error));
        }
    }

    #[test]
    fn listener_binds_ipv4_loopback_ephemeral_port_and_returns_static_html_once() {
        let port = LoopbackCallbackPort::with_timeout(Duration::from_secs(1));
        let mut session = port.bind().expect("bind callback");
        let redirect = session.redirect_uri().to_owned();
        let address = redirect
            .strip_prefix("http://")
            .expect("http redirect")
            .strip_suffix(CALLBACK_PATH)
            .expect("callback path")
            .to_owned();
        let client_authority = address.clone();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(address).expect("connect callback");
            write!(
                stream,
                "GET {CALLBACK_PATH}?code=fixture-code&state={STATE} HTTP/1.1\r\nHost: {client_authority}\r\n\r\n"
            )
            .expect("send callback");
            let mut response = String::new();
            stream.read_to_string(&mut response).expect("read response");
            response
        });
        let attempts = AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let callback =
            tauri::async_runtime::block_on(session.receive(STATE, &guard.cancellation()))
                .expect("receive callback");

        assert_eq!(callback.code.expose(), "fixture-code");
        let response = client.join().expect("client thread");
        assert!(response.contains(CALLBACK_HTML));
        assert!(!response.contains("fixture-code"));
        assert!(!response.contains(STATE));
    }

    #[test]
    fn timeout_and_cancellation_are_bounded_and_sanitized() {
        let attempts = AddAccountAttemptGuardSet::new();
        let guard = attempts.begin().expect("attempt");
        let mut timeout_session = LoopbackCallbackPort::with_timeout(Duration::from_millis(20))
            .bind()
            .expect("bind callback");
        let error =
            tauri::async_runtime::block_on(timeout_session.receive(STATE, &guard.cancellation()))
                .unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::Timeout);

        attempts.cancel_active().expect("cancel active");
        let mut cancelled_session = LoopbackCallbackPort::with_timeout(Duration::from_secs(1))
            .bind()
            .expect("bind callback");
        let error =
            tauri::async_runtime::block_on(cancelled_session.receive(STATE, &guard.cancellation()))
                .unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::Cancelled);
    }

    #[test]
    fn oversized_request_is_rejected() {
        let mut request = format!("GET {CALLBACK_PATH}?code=");
        request.push_str(&"x".repeat(MAX_REQUEST_BYTES));
        request.push_str(&format!("&state={STATE} HTTP/1.1\r\n\r\n"));
        let error = parse_request(
            request.as_bytes(),
            &SensitiveString::new(STATE.into()),
            AUTHORITY,
        )
        .unwrap_err();
        assert_eq!(error.tag(), AddAccountErrorTag::CallbackInvalid);
    }
}
