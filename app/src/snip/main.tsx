import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles.css";
import { Snip } from "./Snip";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Snip />
  </StrictMode>,
);
