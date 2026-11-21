import { test } from '@playwright/test';
import { replay } from '../runner.mjs';

// Shrunk by the browser fuzz from seed 92 on react (?random).
// Late problems on a long text, then a short text of the host's own: the
// host takes them back with it (they name places past its end).
test("react: errors (seed 92)", async ({ browser }) => {
  await replay(browser, "react", "?random", [
    {
      "k": "hostValue",
      "text": "group_0:\n  name: g0\n  items:\n    - one\n    - two\nkey_1: value 1 😀\nkey_2: value 2 😀\nkey_3: value 3 😀\nkey_4: value 4 😀\nkey_5: value 5 😀\nkey_6: value 6 😀\nkey_7: value 7 😀\nkey_8: value 8 😀\nkey_9: value 9 😀\ngroup_10:\n  name: g10\n  items:\n    - one\n    - two\nkey_11: value 11 😀\nkey_12: value 12 😀\nkey_13: value 13 😀\nkey_14: value 14 😀\nkey_15: value 15 😀\nkey_16: value 16 😀\nkey_17: value 17 😀\nkey_18: value 18 😀\nkey_19: value 19 😀\ngroup_20:\n  name: g20\n  items:\n    - one\n    - two\nkey_21: value 21 😀\nkey_22: value 22 😀\nkey_23: value 23 😀\nkey_24: value 24 😀\nkey_25: value 25 😀\nkey_26: value 26 😀\nkey_27: value 27 😀\nkey_28: value 28 😀\nkey_29: value 29 😀\ngroup_30:\n  name: g30\n  items:\n    - one\n    - two\nkey_31: value 31 😀\nkey_32: value 32 😀\nkey_33: value 33 😀\nkey_34: value 34 😀\nkey_35: value 35 😀\nkey_36: value 36 😀\nkey_37: value 37 😀\nkey_38: value 38 😀\nkey_39: value 39 😀\ngroup_40:\n  name: g40\n  items:\n    - one\n    - two\nkey_41: value 41 😀\nkey_42: value 42 😀\nkey_43: value 43 😀\nkey_44: value 44 😀\nkey_45: value 45 😀\nkey_46: value 46 😀\nkey_47: value 47 😀\nkey_48: value 48 😀\nkey_49: value 49 😀\ngroup_50:\n  name: g50\n  items:\n    - one\n    - two\nkey_51: value 51 😀\nkey_52: value 52 😀\nkey_53: value 53 😀\nkey_54: value 54 😀\nkey_55: value 55 😀\nkey_56: value 56 😀\nkey_57: value 57 😀\nkey_58: value 58 😀\nkey_59: value 59 😀\ngroup_60:\n  name: g60\n  items:\n    - one\n    - two\nkey_61: value 61 😀\nkey_62: value 62 😀\nkey_63: value 63 😀\nkey_64: value 64 😀\nkey_65: value 65 😀\nkey_66: value 66 😀\nkey_67: value 67 😀\nkey_68: value 68 😀\nkey_69: value 69 😀\ngroup_70:\n  name: g70\n  items:\n    - one\n    - two\nkey_71: value 71 😀\nkey_72: value 72 😀\nkey_73: value 73 😀\nkey_74: value 74 😀\nkey_75: value 75 😀\nkey_76: value 76 😀\nkey_77: value 77 😀\nkey_78: value 78 😀\nkey_79: value 79 😀\ngroup_80:\n  name: g80\n  items:\n    - one\n    - two\nkey_81: value 81 😀\nkey_82: value 82 😀\nkey_83: value 83 😀\nkey_84: value 84 😀\nkey_85: value 85 😀\nkey_86: value 86 😀\nkey_87: value 87 😀\nkey_88: value 88 😀\nkey_89: value 89 😀\ngroup_90:\n  name: g90\n  items:\n    - one\n    - two\nkey_91: value 91 😀\nkey_92: value 92 😀\nkey_93: value 93 😀\nkey_94: value 94 😀\nkey_95: value 95 😀\nkey_96: value 96 😀\nkey_97: value 97 😀\nkey_98: value 98 😀\nkey_99: value 99 😀\ngroup_100:\n  name: g100\n  items:\n    - one\n    - two\nkey_101: value 101 😀\nkey_102: value 102 😀\nkey_103: value 103 😀\nkey_104: value 104 😀\nkey_105: value 105 😀\nkey_106: value 106 😀\nkey_107: value 107 😀\nkey_108: value 108 😀\nkey_109: value 109 😀\ngroup_110:\n  name: g110\n  items:\n    - one\n    - two\nkey_111: value 111 😀\nkey_112: value 112 😀\nkey_113: value 113 😀\nkey_114: value 114 😀\nkey_115: value 115 😀\nkey_116: value 116 😀\nkey_117: value 117 😀\nkey_118: value 118 😀\nkey_119: value 119 😀\ngroup_120:\n  name: g120\n  items:\n    - one\n    - two\nkey_121: value 121 😀\nkey_122: value 122 😀\nkey_123: value 123 😀\nkey_124: value 124 😀\nkey_125: value 125 😀\nkey_126: value 126 😀\nkey_127: value 127 😀\nkey_128: value 128 😀\nkey_129: value 129 😀\ngroup_130:\n  name: g130\n  items:\n    - one\n    - two\nkey_131: value 131 😀\nkey_132: value 132 😀\nkey_133: value 133 😀\nkey_134: value 134 😀\nkey_135: value 135 😀\nkey_136: value 136 😀\nkey_137: value 137 😀\nkey_138: value 138 😀\nkey_139: value 139 😀\ngroup_140:\n  name: g140\n  items:\n    - one\n    - two\nkey_141: value 141 😀\nkey_142: value 142 😀\nkey_143: value 143 😀\nkey_144: value 144 😀\nkey_145: value 145 😀\nkey_146: value 146 😀\nkey_147: value 147 😀\nkey_148: value 148 😀\nkey_149: value 149 😀\ngroup_150:\n  name: g150\n  items:\n    - one\n    - two\nkey_151: value 151 😀\nkey_152: value 152 😀\nkey_153: value 153 😀\nkey_154: value 154 😀\nkey_155: value 155 😀\nkey_156: value 156 😀\nkey_157: value 157 😀\nkey_158: value 158 😀\nkey_159: value 159 😀\n",
      "how": "prop"
    },
    {
      "k": "lateDiagnostics",
      "at": 0.527,
      "by": "revision",
      "wait": 200
    },
    {
      "k": "hostValue",
      "text": "server:\n  host: localhost\n  ports:\n    - 80\n    - 443\n  tls:\n    enabled: true\nname: 日本 😀\n",
      "how": "prop"
    }
  ], "errors");
});
