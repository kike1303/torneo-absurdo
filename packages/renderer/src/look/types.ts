// What a fighter wears (022): data only, no Three. The host builds it from content/models/.

/** Color slots of an accessory's materials (`extras.tint_slot` in its GLB). */
export const TINT_SLOTS = ['primary', 'secondary', 'accent'] as const;
export type TintSlot = (typeof TINT_SLOTS)[number];

/** Body measurements an accessory is scaled and centered by. */
export const BODY_SLOTS = ['head', 'hands', 'feet', 'hips', 'chest'] as const;
export type BodySlot = (typeof BODY_SLOTS)[number];

export type Vec3 = readonly [number, number, number];

/** How a body differs from the base botarga at one slot (glTF axes). */
export interface SlotFit {
  readonly scale: Vec3;
  readonly offset: Vec3;
}
export type BodyFit = Readonly<Partial<Record<BodySlot, SlotFit>>>;

export interface AccessoryAttach {
  /** Bone it hangs from, by the end of its name (`Head`, `LeftHand`). */
  readonly bone: string;
  /** Bone whose rest position it is placed at; the same bone without it. */
  readonly anchor?: string;
  /** The right-hand piece of a pair modeled for the left. */
  readonly mirror?: boolean;
  /** Only this node of the GLB hangs here (a pair whose two pieces differ); the whole scene without it. */
  readonly node?: string;
}

/** Fine placement on the base botarga: meters, a uniform scale and a forward tilt in degrees. */
export interface AccessoryFit {
  readonly y?: number;
  readonly z?: number;
  readonly scale?: number;
  readonly tilt?: number;
}

/** `accessory.json`, next to the GLB. */
export interface AccessoryManifest {
  readonly slot: BodySlot;
  readonly attach: readonly AccessoryAttach[];
  readonly fit?: AccessoryFit;
  /** The body skin it covers is not drawn. */
  readonly hides?: boolean;
}

export interface ColorZone {
  readonly id: string;
  /** Mean color of the zone in the texture, `#RRGGBB`. */
  readonly color: string;
  /** Takes the shift of another zone (its shaded part). */
  readonly follows?: string;
  /** Only moves on a texture drawn by one of these materials. */
  readonly only?: readonly string[];
}

/** A patch that shares its color with a zone but keeps its own (a tongue). */
export interface ZoneIsland {
  readonly id: string;
  readonly zone: string;
}

export interface ColorZones {
  readonly zones: readonly ColorZone[];
  readonly islands?: readonly ZoneIsland[];
}

/** Color by zone or island id, `#RRGGBB`. */
export type ColorChoice = Readonly<Record<string, string>>;
