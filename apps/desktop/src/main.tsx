import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import QuitGuard from "./QuitGuard";
import "./styles.css";

ReactDOM.createRoot(document.getElementById("root")!).render(<React.StrictMode><App /><QuitGuard /></React.StrictMode>);
