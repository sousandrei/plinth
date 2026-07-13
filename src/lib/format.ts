export const fmtMajor = (value: number, currency: string): string =>
  new Intl.NumberFormat('sv-SE', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(value);

export const fmtMajorOrDash = (value: number, currency: string): string =>
  value === 0 ? '—' : fmtMajor(value, currency);

export const fmtMinor = (minor: number, currency: string): string =>
  fmtMajor(minor / 100, currency);

export const fmtMonth = (ym: string): string => {
  const parts = ym.split('-');
  if (parts.length < 2) return ym;
  const year = Number(parts[0]);
  const month = Number(parts[1]);
  if (Number.isNaN(year) || Number.isNaN(month)) return ym;
  return new Date(year, month - 1).toLocaleString(undefined, {
    month: 'short',
    year: '2-digit',
  });
};
