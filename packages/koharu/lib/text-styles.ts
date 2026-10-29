import type { ShapeStyles, TextStyle } from '@koharu/bridge/protocol'

export const emptyTextStyle: TextStyle = {
  font: null,
  size: null,
  fill: null,
  outline: null,
  outer_outline: null,
  alignment: null,
  writing_mode: null,
}
export const emptyShapeStyles = {
  enabled: false,
  rectangle: { ...emptyTextStyle },
  round: { ...emptyTextStyle },
  spiky: { ...emptyTextStyle },
  free: { ...emptyTextStyle },
} satisfies ShapeStyles
