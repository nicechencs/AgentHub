/** Generation check shared by usage/filter/event loads. */
export function isLatestUsageRequest(currentGeneration: number, requestGeneration: number) {
  return currentGeneration === requestGeneration;
}
