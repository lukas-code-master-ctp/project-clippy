import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Bubble from "./Bubble";
import "./styles.css";

// La misma página sirve a las dos ventanas: Brote y su globo (index.html#bubble).
const isBubble = window.location.hash === "#bubble";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{isBubble ? <Bubble /> : <App />}</React.StrictMode>,
);
