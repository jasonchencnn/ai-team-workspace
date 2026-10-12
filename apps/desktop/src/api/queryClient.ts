import { QueryClient } from "@tanstack/react-query";

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      // 后端错误是确定性的业务/环境问题，重试没有意义；页面显示 ErrorBanner 并提供手动重试。
      queries: { retry: false, refetchOnWindowFocus: false },
    },
  });
}
