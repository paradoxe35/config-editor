import type { EditorSettings } from "@pngwasi/config-editor";
import { SETTING_TYPES } from "@pngwasi/config-editor/binding";
import type { PropType } from "vue";

/** A setting's prop: typed, and `undefined` when not passed (its default then applies, not `false`). */
type SettingProp<K extends keyof EditorSettings> = {
  type: PropType<EditorSettings[K]>;
  default: undefined;
};

/** One prop per setting, from the list generated from the Rust struct (`SETTING_TYPES`), so they never drift. */
export type SettingProps = { [K in keyof EditorSettings]-?: SettingProp<K> };

const CONSTRUCTORS = { boolean: Boolean, string: String, object: Object } as const;

/**
 * The runtime props of every setting. `default: undefined` keeps an absent
 * boolean `undefined` (Vue would cast it to `false`), while
 * `<ConfigEditor show-footer />` still reads as `true`.
 */
export const settingProps = Object.fromEntries(
  Object.entries(SETTING_TYPES).map(([key, type]) => [key, { type: CONSTRUCTORS[type], default: undefined }]),
) as unknown as SettingProps;
