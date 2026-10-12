/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** 仅 e2e 模式使用：测试桥接的 HTTP 地址。 */
  readonly VITE_E2E_BRIDGE?: string;
}
