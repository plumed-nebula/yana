declare module '@tauri-apps/plugin-dialog' {
  export function open(options?: {
    multiple?: boolean;
    directory?: boolean;
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }): Promise<string | string[] | null>;
  export function save(options?: {
    filters?: { name: string; extensions: string[] }[];
    defaultPath?: string;
  }): Promise<string | null>;
}

declare module '@tauri-apps/plugin-fs' {
  export function copyFile(from: string, to: string): Promise<void>;
}
