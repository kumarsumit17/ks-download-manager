import "./index.css";
import React, { useEffect, useState } from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { api } from "@/features/downloads/api";
import { currentLanguage, onLanguage, setLanguage } from "@/lib/i18n";

// The window is drawn again from the top when the language changes, so every text
// (including text worked out once per render) is in the new language.
function Root() {
  const [language, setShown] = useState(currentLanguage());
  useEffect(() => onLanguage(setShown), []);
  return <App key={language} />;
}

// Light/dark follows the system through CSS (prefers-color-scheme); no script needed.
// The saved language is read first, so the window never shows the other language.
api.settings().then(
  (s) => setLanguage(s.language),
  () => setLanguage("system"),
).finally(() => {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <Root />
    </React.StrictMode>,
  );
});
