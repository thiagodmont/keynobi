export type QueryVariableValues = Record<string, string>;

const QUERY_VARIABLE_NAME_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;
const QUERY_VARIABLE_RE = /\$\{([A-Za-z_][A-Za-z0-9_]*)\}/g;

export function isValidQueryVariableName(name: string): boolean {
  return QUERY_VARIABLE_NAME_RE.test(name);
}

export function extractQueryVariables(query: string): string[] {
  const seen = new Set<string>();
  const names: string[] = [];

  for (const match of query.matchAll(QUERY_VARIABLE_RE)) {
    const name = match[1];
    if (!seen.has(name)) {
      seen.add(name);
      names.push(name);
    }
  }

  return names;
}

export function resolveQueryVariables(query: string, values: QueryVariableValues): string {
  return query.replace(QUERY_VARIABLE_RE, (raw, name: string) => {
    const value = values[name]?.trim();
    return value ? value : raw;
  });
}

export function reconcileQueryVariableValues(
  query: string,
  values: QueryVariableValues
): QueryVariableValues {
  const next: QueryVariableValues = {};
  for (const name of extractQueryVariables(query)) {
    if (values[name] !== undefined) next[name] = values[name];
  }
  return next;
}

export function ensureQueryVariableValues(
  query: string,
  values: QueryVariableValues
): QueryVariableValues {
  const next: QueryVariableValues = { ...values };
  for (const name of extractQueryVariables(query)) {
    if (next[name] === undefined) next[name] = "";
  }
  return next;
}
