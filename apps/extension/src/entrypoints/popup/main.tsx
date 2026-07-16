import { i18n } from "@tabstride/i18n";
import { I18nextProvider } from "@tabstride/i18n/react";
import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./App";
import "./style.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("[tabstride] popup root element missing");
}

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <I18nextProvider i18n={i18n}>
      <App />
    </I18nextProvider>
  </React.StrictMode>,
);
