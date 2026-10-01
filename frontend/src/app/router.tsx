import { createBrowserRouter } from "react-router-dom";
import { AppLayout } from "@/shared/layout/AppLayout";
import OverviewPage from "@/features/overview/OverviewPage";
import BscPage from "@/features/bsc/BscPage";
import Project0Page from "@/features/project0/Project0Page";
import SolanaPage from "@/features/solana/SolanaPage";

export const router = createBrowserRouter([
  { path: "/", element: <AppLayout />, children: [
    { index: true, element: <OverviewPage /> },
    { path: "bsc", element: <BscPage /> },
    { path: "project-0", element: <Project0Page /> },
    { path: "solana", element: <SolanaPage /> },
  ]},
]);
