import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./styles/hud.css";

const qc = new QueryClient({
  defaultOptions: {
    // The watcher drives refetches; nothing goes stale on its own.
    queries: { retry: false, refetchOnWindowFocus: false, staleTime: Infinity },
  },
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={qc}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
