import "./i18n"; // must be imported before any component that calls useTranslation()
import React from "react";
import ReactDOM from "react-dom/client";
import LoanStatusDashboard from "./LoanStatusDashboard";

const root = document.getElementById("root");
if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <LoanStatusDashboard
        borrower={import.meta.env.VITE_BORROWER ?? ""}
        wsUrl={import.meta.env.VITE_WS_URL ?? "http://localhost:3000"}
        apiKey={import.meta.env.VITE_API_KEY}
      />
    </React.StrictMode>,
  );
}
