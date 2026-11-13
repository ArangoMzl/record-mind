import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles.css";
import { Capture } from "./Capture";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Capture />
  </StrictMode>,
);
