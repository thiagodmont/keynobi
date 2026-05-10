/** Set by the app on project open. Used to resolve `package:mine`. */
let minePackage: string | null = null;

export function setMinePackage(pkg: string | null): void {
  minePackage = pkg;
}

export function getMinePackage(): string | null {
  return minePackage;
}
