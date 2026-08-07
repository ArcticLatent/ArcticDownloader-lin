// Ambient typing for the Tauri IPC bridge injected into the webview at
// runtime (`withGlobalTauri` in tauri.conf.json). Not a real dependency --
// there's nothing to `npm install` for this global, so it's declared here
// as `any` rather than modeled in full: this repo cares about type safety
// for our own frontend code, not about being a complete Tauri API surface.
export {};

declare global {
  interface Window {
    __TAURI__?: {
      core?: {
        invoke?: (...args: any[]) => Promise<any>;
        listen?: (...args: any[]) => any;
      };
      event?: {
        listen?: (...args: any[]) => any;
      };
      [key: string]: any;
    };
  }
}
