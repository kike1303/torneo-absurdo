// Kit normalizes trailing slashes before `handle` runs (default `never`), which would answer
// `/es/` with a 308 to `/es`. RF-A3 redirects to `/<locale>/`, so both forms are served as-is.
export const trailingSlash = 'ignore';
