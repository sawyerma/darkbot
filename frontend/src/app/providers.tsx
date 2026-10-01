import type { PropsWithChildren } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import ThemeProvider from "@/shared/ui/theme-provider";

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: 1, refetchOnWindowFocus: false } } });

export function AppProviders({ children }: PropsWithChildren) {
  return <ThemeProvider><QueryClientProvider client={queryClient}>{children}</QueryClientProvider></ThemeProvider>;
}
