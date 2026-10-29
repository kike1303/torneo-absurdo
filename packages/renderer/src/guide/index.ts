// @torneo/renderer/guide: the sprite guide layer (007 RF-A2). Pure, no Pixi and no DOM, and never
// imported by the runtime of the renderer (test/guide-imports.test.ts).
export { GUIDE_LIMITS } from './limits.ts';
export { GUIDE_CODES, RULESET_CODES } from './codes.ts';
export {
  parseRulesetTiming,
  type RulesetError,
  type RulesetResult,
  type RulesetTiming,
} from './ruleset.ts';
export {
  guideTable,
  windupCap,
  type GuideRange,
  type GuideRow,
  type WindupRange,
} from './table.ts';
export {
  validateGuide,
  type GuideError,
  type GuideErrorCode,
  type GuideResult,
  type RgbaImage,
} from './validate.ts';
