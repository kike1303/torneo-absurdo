// AIR text writer (002 RF-B6).
export interface AirFrameSpec {
  readonly group: number;
  readonly image: number;
  readonly x?: number;
  readonly y?: number;
  readonly ticks: number;
  readonly flip?: '' | 'H' | 'V' | 'HV';
}

export interface AirActionSpec {
  readonly id: number;
  readonly frames: readonly AirFrameSpec[];
  readonly loopStart?: number;
}

export function writeAir(actions: readonly AirActionSpec[]): string {
  const lines: string[] = ['; synthetic AIR'];
  for (const action of actions) {
    lines.push(`[Begin Action ${action.id}]`, 'Clsn2Default: 1', ' Clsn2[0] = -10, 0, 10, -60');
    action.frames.forEach((f, i) => {
      if (action.loopStart === i) lines.push('Loopstart');
      const fields = [f.group, f.image, f.x ?? 0, f.y ?? 0, f.ticks].join(', ');
      lines.push(f.flip ? `${fields}, ${f.flip}` : fields);
    });
    lines.push('');
  }
  return lines.join('\r\n');
}
