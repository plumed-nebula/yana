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
  export enum BaseDirectory {
    Home = 0,
    AppCache = 1,
    AppConfig = 2,
    AppData = 3,
    AppLog = 4,
    Audio = 5,
    Cache = 6,
    Config = 7,
    Data = 8,
    LocalData = 9,
    Document = 10,
    Download = 11,
    Executable = 12,
    Font = 13,
    Library = 14,
    Pictures = 15,
    Public = 16,
    Runtime = 17,
    Temp = 18,
    Video = 19,
    Resource = 20,
    App = 21,
  }

  export interface CopyFileOptions {
    dir?: BaseDirectory;
  }

  export function copyFile(
    from: string,
    to: string,
    options?: CopyFileOptions
  ): Promise<void>;

  export function readFile(
    path: string,
    options?: { dir?: BaseDirectory }
  ): Promise<Uint8Array>;
  export function writeFile(
    path: string,
    contents: Uint8Array | string,
    options?: { dir?: BaseDirectory }
  ): Promise<void>;
}
