import type { AppSettings } from '@/lib/types';
import type { zh } from './locales/zh';

export type UiLanguage = AppSettings['language'];

/** 叶子放宽为 string，避免 en 被 zh 的中文字面量锁死。 */
type StringifyLeaves<T> = T extends string
  ? string
  : T extends object
    ? { [K in keyof T]: StringifyLeaves<T[K]> }
    : T;

export type Dict = StringifyLeaves<typeof zh>;

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
