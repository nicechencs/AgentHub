/** Enter confirms. Shift+Enter and IME composition must not. */
export function dialogEnterShouldConfirm(input: {
  key: string;
  shiftKey: boolean;
  isComposing?: boolean;
  nativeEvent?: { isComposing?: boolean; keyCode?: number };
}): boolean {
  if (input.key !== 'Enter' || input.shiftKey) return false;
  if (input.isComposing || input.nativeEvent?.isComposing) return false;
  if (input.nativeEvent?.keyCode === 229) return false;
  return true;
}
