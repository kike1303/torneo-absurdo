// The id of a season in a URL (020): exactly `YYYYMM`, a year from 2000 and a month from 01 to 12,
// the same rule as the server. Only what passes here reaches a read, and it does so as a number.

const SHAPE = /^[0-9]{6}$/;

export function parseSeasonId(text: string): number | null {
  if (!SHAPE.test(text)) return null;
  const id = Number(text);
  const year = Math.floor(id / 100);
  const month = id % 100;
  return year >= 2000 && month >= 1 && month <= 12 ? id : null;
}
