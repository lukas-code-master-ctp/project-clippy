import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Bubble from "./Bubble";
import Confirm from "./Confirm";
import Settings from "./Settings";
import "./styles.css";

// La misma página sirve a todas las ventanas; el hash dice cuál es.
const views: Record<string, () => React.JSX.Element | null> = {
  "#bubble": Bubble,
  "#confirm": Confirm,
  "#settings": Settings,
};
const View = views[window.location.hash] ?? App;
if (window.location.hash === "#settings") document.documentElement.classList.add("opaque");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <View />
  </React.StrictMode>,
);
