import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "@/App";
import { SoloPane } from "@/components/SoloPane";
import { CoreStartup } from "@/components/CoreStartup";
import "@/index.css";

const container = document.getElementById("root");
if (!container) {
    throw new Error("root element is missing");
}

const solo = new URLSearchParams(window.location.search).get("pane");

createRoot(container).render(
    <StrictMode>
        <CoreStartup>{solo ? <SoloPane session_id={solo} /> : <App />}</CoreStartup>
    </StrictMode>,
);
