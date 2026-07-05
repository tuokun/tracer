/** 去掉 .exe 后缀（大小写不敏感）。 */
export function stripExe(name: string): string {
  return name.replace(/\.exe$/i, '');
}

/**
 * 应用显示名：优先 display_name，回退到去掉 .exe 后缀的 process_name。
 * 用于所有面向用户的应用名展示。
 */
export function appDisplayName(displayName: string | null | undefined, processName: string): string {
  return stripExe(displayName ?? processName);
}

