/** Split `~/.workbuddy/models.json` into directory prefix + file name. */
export function splitFileLabel(path: string, name = ''): {
  directory: string;
  fileName: string;
} {
  const fileName = name.trim() || baseName(path);
  const pathTrim = path.trim();
  if (!pathTrim) return { directory: '', fileName };
  if (
    pathTrim === fileName
    || pathTrim.endsWith(`/${fileName}`)
    || pathTrim.endsWith(`\\${fileName}`)
  ) {
    return { directory: pathTrim.slice(0, pathTrim.length - fileName.length), fileName };
  }
  const slash = Math.max(pathTrim.lastIndexOf('/'), pathTrim.lastIndexOf('\\'));
  if (slash >= 0) {
    return {
      directory: pathTrim.slice(0, slash + 1),
      fileName: pathTrim.slice(slash + 1),
    };
  }
  return { directory: '', fileName: pathTrim };
}

function baseName(path: string): string {
  const slash = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return slash >= 0 ? path.slice(slash + 1) : path;
}
