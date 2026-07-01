import React from "react";
import ReactDOM from "react-dom/client";
import LoanStatusDashboard from "./LoanStatusDashboard";

const root = document.getElementById("root");

if (root) {
  ReactDOM.createRoot(root).render(
    <React.StrictMode>
      <LoanStatusDashboard borrower="GABC1234BORROWER" wsUrl="http://localhost:3000" />
    </React.StrictMode>
  );
}
