import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "../styles.css";
import { BoardApp } from "./BoardApp";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BoardApp />
  </StrictMode>,
);
