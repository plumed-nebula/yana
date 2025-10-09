import { invoke } from '@tauri-apps/api/core';

export interface GalleryItem {
  id: number;
  file_name: string;
  url: string;
  host: string;
  delete_marker?: string | null;
  inserted_at: string;
  filesize?: number | null;
}

export interface NewGalleryItem {
  file_name: string;
  url: string;
  host: string;
  delete_marker?: string | null;
  inserted_at?: string;
  filesize?: number | null;
}

export interface GalleryQuery {
  file_name?: string;
  host?: string;
  start_utc?: string;
  end_utc?: string;
  min_filesize?: number;
  max_filesize?: number;
}

export const insertGalleryItem = (item: NewGalleryItem) =>
  invoke<GalleryItem>('gallery_insert_item', { item });

export const deleteGalleryItem = (id: number) =>
  invoke<void>('gallery_delete_item', { id });

export const queryGalleryItems = (query?: GalleryQuery) =>
  invoke<GalleryItem[]>('gallery_query_items', { query });

export const listGalleryHosts = () => invoke<string[]>('gallery_list_hosts');
