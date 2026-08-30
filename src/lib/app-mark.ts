/** Shared product-mark SVG used by the in-app logo rasterizer (window / tray). */

export const APP_MARK_PATH =
  'M300 682 430 342h164l130 340M354 548h316';

export function appMarkSvg(fill: string, size = 128): string {
  // width/height are required: WebKit (macOS / Linux) treats viewBox-only SVG
  // images as 0×0, so canvas.drawImage would bake a blank tray/window icon.
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024" width="${size}" height="${size}">
  <rect width="1024" height="1024" rx="224" fill="${fill}"/>
  <path d="${APP_MARK_PATH}" fill="none" stroke="#fff" stroke-width="78" stroke-linecap="round" stroke-linejoin="round"/>
  <circle cx="300" cy="682" r="52" fill="#fff"/>
  <circle cx="512" cy="548" r="52" fill="#fff"/>
  <circle cx="724" cy="682" r="52" fill="#fff"/>
</svg>`;
}
