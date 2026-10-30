import { QueryClientProvider } from "@tanstack/react-query";
import { HotkeysProvider } from "@tanstack/react-hotkeys";
import { RouterProvider } from "@tanstack/react-router";
import { MotionConfig } from "motion/react";
import React from "react";
import ReactDOM from "react-dom/client";
import { queryClient, useBackendEvents } from "./lib/api";
import { router } from "./router";
import "./index.css";

function App() {
  useBackendEvents();
  return <RouterProvider router={router} />;
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <HotkeysProvider>
        {/* Reduce Motion in System Settings: Motion drops movement, keeps fades. */}
        <MotionConfig reducedMotion="user">
          <App />
        </MotionConfig>
      </HotkeysProvider>
    </QueryClientProvider>
  </React.StrictMode>,
);
