import type { AppSettings } from '@/lib/types';
import type { zh } from './locales/zh';

export type UiLanguage = AppSettings['language'];

export type Dict = typeof zh;

export type MessageParams = Record<string, string | number>;

type NestedKeyOf<T> = T extends object
  ? {
      [K in keyof T & string]: T[K] extends string
        ? K
        : T[K] extends object
          ? `${K}.${NestedKeyOf<T[K]>}`
          : never;
    }[keyof T & string]
  : never;

export type MessageKey = NestedKeyOf<Dict>;

export type TranslateFn = (key: MessageKey, params?: MessageParams) => string;
