import "./App.css";
import { AccountList } from "./components/AccountList";
import { PetRenderer } from "./pet/PetRenderer";
import { presentPet } from "./pet/PetEngine";
import { mockAccounts } from "./stores/mockState";

function App() {
  const pendingTotal = mockAccounts.reduce((total, account) => total + account.pendingCount, 0);
  const presentation = presentPet({
    pendingTotal,
    accounts: mockAccounts.map(({ id, pendingCount }) => ({ accountId: id, pendingCount })),
  });

  return (
    <main className="app-shell">
      <header className="app-header">
        <p className="eyebrow">YOUR INBOX PET</p>
        <h1>ZEROCHI</h1>
      </header>
      <PetRenderer presentation={presentation} visible />
      <section className="summary" aria-live="polite">
        <strong>Tengo {pendingTotal} mails</strong>
        <span>{presentation.message}</span>
      </section>
      <AccountList accounts={mockAccounts} />
      <footer className="app-footer">
        <button type="button">＋ Agregar cuenta</button>
        <button type="button" aria-label="Abrir configuración">⚙︎</button>
      </footer>
    </main>
  );
}

export default App;
