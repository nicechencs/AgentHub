import table from './source-classify-contract.json';

export type ClassifyProductId = (typeof table.products)[number];

export const SOURCE_CLASSIFY_CONTRACT = table;

export function productFromMockSource(id: string | null): ClassifyProductId {
  if (!id) return 'other';
  const product = table.mockSourceToProduct[id as keyof typeof table.mockSourceToProduct];
  return (product ?? 'other') as ClassifyProductId;
}
