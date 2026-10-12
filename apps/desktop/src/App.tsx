import { QueryClientProvider } from "@tanstack/react-query";
import { useState } from "react";
import { HashRouter } from "react-router";
import { createQueryClient } from "./api/queryClient.js";
import { AppRoutes } from "./routes.js";

export function App() {
  const [queryClient] = useState(createQueryClient);
  return (
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <AppRoutes />
      </HashRouter>
    </QueryClientProvider>
  );
}
