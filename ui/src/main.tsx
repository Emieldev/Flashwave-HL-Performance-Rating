import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./index.css";
import { initTheme } from "./lib/theme";
import { loadUserLanguages } from "./lib/userLang";

// Local IPC, not a network: refetching on every window focus buys nothing.
const queryClient = new QueryClient({
  defaultOptions: {
    queries: { refetchOnWindowFocus: false, staleTime: 5_000 },
  },
});

// Before the first render, so the window never paints the default palette
// and then swaps to the chosen one.
initTheme();

// Translation fixes from the user's lang folder. Not awaited: the built-in
// text paints first, and the fixes swap in the moment they are read.
void loadUserLanguages();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
