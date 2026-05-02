/**
 * Instrument definitions for the music module.
 *
 * The selector exposes the full 128 General MIDI SoundFont instruments that
 * `smplr` can play, plus the built-in modeled synth piano that works offline.
 */

// ── Types ───────────────────────────────────────────────────────────

export interface InstrumentCategory {
  id: string;
  label: { zh: string; en: string };
}

export interface InstrumentDef {
  /** Unique identifier used as store key. For SoundFont instruments this must
   *  match the instrument name recognised by `smplr`. */
  id: string;
  /** Display labels. */
  label: { zh: string; en: string };
  /** Category the instrument belongs to. */
  categoryId: string;
  /** When true, the instrument is the built-in modeled synth (no network). */
  builtIn?: true;
}

interface InstrumentSeed {
  id: string;
  label: { zh: string; en: string };
}

interface InstrumentGroup {
  categoryId: string;
  instruments: InstrumentSeed[];
}

// ── Categories ──────────────────────────────────────────────────────

export const INSTRUMENT_CATEGORIES: InstrumentCategory[] = [
  { id: 'piano', label: { zh: '钢琴', en: 'Piano' } },
  { id: 'chromatic', label: { zh: '色彩打击乐', en: 'Chromatic Percussion' } },
  { id: 'organ', label: { zh: '风琴 / 手风琴', en: 'Organ / Accordion' } },
  { id: 'guitar', label: { zh: '吉他', en: 'Guitar' } },
  { id: 'bass', label: { zh: '贝斯', en: 'Bass' } },
  { id: 'strings', label: { zh: '弦乐', en: 'Strings' } },
  { id: 'ensemble', label: { zh: '合奏 / 合唱', en: 'Ensemble / Choir' } },
  { id: 'brass', label: { zh: '铜管', en: 'Brass' } },
  { id: 'reed', label: { zh: '簧管', en: 'Reed' } },
  { id: 'pipe', label: { zh: '笛箫 / 管乐', en: 'Pipe / Wind' } },
  { id: 'synth-lead', label: { zh: '合成主音', en: 'Synth Lead' } },
  { id: 'synth-pad', label: { zh: '合成铺底', en: 'Synth Pad' } },
  { id: 'synth-fx', label: { zh: '合成效果', en: 'Synth Effects' } },
  { id: 'ethnic', label: { zh: '民族乐器', en: 'Ethnic' } },
  { id: 'percussive', label: { zh: '打击乐', en: 'Percussive' } },
  { id: 'sfx', label: { zh: '音效', en: 'Sound Effects' } },
];

const GM_INSTRUMENT_GROUPS: InstrumentGroup[] = [
  {
    categoryId: 'piano',
    instruments: [
      { id: 'acoustic_grand_piano', label: { zh: '大钢琴', en: 'Acoustic Grand Piano' } },
      { id: 'bright_acoustic_piano', label: { zh: '亮音大钢琴', en: 'Bright Acoustic Piano' } },
      { id: 'electric_grand_piano', label: { zh: '电钢琴', en: 'Electric Grand Piano' } },
      { id: 'honkytonk_piano', label: { zh: '酒吧钢琴', en: 'Honky-tonk Piano' } },
      { id: 'electric_piano_1', label: { zh: '电钢琴1', en: 'Electric Piano 1' } },
      { id: 'electric_piano_2', label: { zh: '电钢琴2', en: 'Electric Piano 2' } },
      { id: 'harpsichord', label: { zh: '羽管键琴', en: 'Harpsichord' } },
      { id: 'clavinet', label: { zh: '击弦电钢琴', en: 'Clavinet' } },
    ],
  },
  {
    categoryId: 'chromatic',
    instruments: [
      { id: 'celesta', label: { zh: '钢片琴', en: 'Celesta' } },
      { id: 'glockenspiel', label: { zh: '钟琴', en: 'Glockenspiel' } },
      { id: 'music_box', label: { zh: '八音盒', en: 'Music Box' } },
      { id: 'vibraphone', label: { zh: '电颤琴', en: 'Vibraphone' } },
      { id: 'marimba', label: { zh: '马林巴', en: 'Marimba' } },
      { id: 'xylophone', label: { zh: '木琴', en: 'Xylophone' } },
      { id: 'tubular_bells', label: { zh: '管钟', en: 'Tubular Bells' } },
      { id: 'dulcimer', label: { zh: '扬琴', en: 'Dulcimer' } },
    ],
  },
  {
    categoryId: 'organ',
    instruments: [
      { id: 'drawbar_organ', label: { zh: '拉杆风琴', en: 'Drawbar Organ' } },
      { id: 'percussive_organ', label: { zh: '打击风琴', en: 'Percussive Organ' } },
      { id: 'rock_organ', label: { zh: '摇滚风琴', en: 'Rock Organ' } },
      { id: 'church_organ', label: { zh: '管风琴', en: 'Church Organ' } },
      { id: 'reed_organ', label: { zh: '簧风琴', en: 'Reed Organ' } },
      { id: 'accordion', label: { zh: '手风琴', en: 'Accordion' } },
      { id: 'harmonica', label: { zh: '口琴', en: 'Harmonica' } },
      { id: 'tango_accordion', label: { zh: '探戈手风琴', en: 'Tango Accordion' } },
    ],
  },
  {
    categoryId: 'guitar',
    instruments: [
      { id: 'acoustic_guitar_nylon', label: { zh: '尼龙弦吉他', en: 'Acoustic Guitar (Nylon)' } },
      { id: 'acoustic_guitar_steel', label: { zh: '钢弦吉他', en: 'Acoustic Guitar (Steel)' } },
      { id: 'electric_guitar_jazz', label: { zh: '爵士电吉他', en: 'Electric Guitar (Jazz)' } },
      { id: 'electric_guitar_clean', label: { zh: '清音电吉他', en: 'Electric Guitar (Clean)' } },
      { id: 'electric_guitar_muted', label: { zh: '闷音电吉他', en: 'Electric Guitar (Muted)' } },
      { id: 'overdriven_guitar', label: { zh: '过载吉他', en: 'Overdriven Guitar' } },
      { id: 'distortion_guitar', label: { zh: '失真吉他', en: 'Distortion Guitar' } },
      { id: 'guitar_harmonics', label: { zh: '吉他泛音', en: 'Guitar Harmonics' } },
    ],
  },
  {
    categoryId: 'bass',
    instruments: [
      { id: 'acoustic_bass', label: { zh: '原声贝斯', en: 'Acoustic Bass' } },
      { id: 'electric_bass_finger', label: { zh: '指弹电贝斯', en: 'Electric Bass (Finger)' } },
      { id: 'electric_bass_pick', label: { zh: '拨片电贝斯', en: 'Electric Bass (Pick)' } },
      { id: 'fretless_bass', label: { zh: '无品贝斯', en: 'Fretless Bass' } },
      { id: 'slap_bass_1', label: { zh: '击勾贝斯1', en: 'Slap Bass 1' } },
      { id: 'slap_bass_2', label: { zh: '击勾贝斯2', en: 'Slap Bass 2' } },
      { id: 'synth_bass_1', label: { zh: '合成贝斯1', en: 'Synth Bass 1' } },
      { id: 'synth_bass_2', label: { zh: '合成贝斯2', en: 'Synth Bass 2' } },
    ],
  },
  {
    categoryId: 'strings',
    instruments: [
      { id: 'violin', label: { zh: '小提琴', en: 'Violin' } },
      { id: 'viola', label: { zh: '中提琴', en: 'Viola' } },
      { id: 'cello', label: { zh: '大提琴', en: 'Cello' } },
      { id: 'contrabass', label: { zh: '低音提琴', en: 'Contrabass' } },
      { id: 'tremolo_strings', label: { zh: '颤音弦乐', en: 'Tremolo Strings' } },
      { id: 'pizzicato_strings', label: { zh: '拨奏弦乐', en: 'Pizzicato Strings' } },
      { id: 'orchestral_harp', label: { zh: '竖琴', en: 'Orchestral Harp' } },
      { id: 'timpani', label: { zh: '定音鼓', en: 'Timpani' } },
    ],
  },
  {
    categoryId: 'ensemble',
    instruments: [
      { id: 'string_ensemble_1', label: { zh: '弦乐合奏1', en: 'String Ensemble 1' } },
      { id: 'string_ensemble_2', label: { zh: '弦乐合奏2', en: 'String Ensemble 2' } },
      { id: 'synth_strings_1', label: { zh: '合成弦乐1', en: 'Synth Strings 1' } },
      { id: 'synth_strings_2', label: { zh: '合成弦乐2', en: 'Synth Strings 2' } },
      { id: 'choir_aahs', label: { zh: '合唱“啊”', en: 'Choir Aahs' } },
      { id: 'voice_oohs', label: { zh: '人声“呜”', en: 'Voice Oohs' } },
      { id: 'synth_choir', label: { zh: '合成人声', en: 'Synth Choir' } },
      { id: 'orchestra_hit', label: { zh: '乐队重击', en: 'Orchestra Hit' } },
    ],
  },
  {
    categoryId: 'brass',
    instruments: [
      { id: 'trumpet', label: { zh: '小号', en: 'Trumpet' } },
      { id: 'trombone', label: { zh: '长号', en: 'Trombone' } },
      { id: 'tuba', label: { zh: '大号', en: 'Tuba' } },
      { id: 'muted_trumpet', label: { zh: '弱音小号', en: 'Muted Trumpet' } },
      { id: 'french_horn', label: { zh: '圆号', en: 'French Horn' } },
      { id: 'brass_section', label: { zh: '铜管乐组', en: 'Brass Section' } },
      { id: 'synth_brass_1', label: { zh: '合成铜管1', en: 'Synth Brass 1' } },
      { id: 'synth_brass_2', label: { zh: '合成铜管2', en: 'Synth Brass 2' } },
    ],
  },
  {
    categoryId: 'reed',
    instruments: [
      { id: 'soprano_sax', label: { zh: '高音萨克斯', en: 'Soprano Sax' } },
      { id: 'alto_sax', label: { zh: '中音萨克斯', en: 'Alto Sax' } },
      { id: 'tenor_sax', label: { zh: '次中音萨克斯', en: 'Tenor Sax' } },
      { id: 'baritone_sax', label: { zh: '上低音萨克斯', en: 'Baritone Sax' } },
      { id: 'oboe', label: { zh: '双簧管', en: 'Oboe' } },
      { id: 'english_horn', label: { zh: '英国管', en: 'English Horn' } },
      { id: 'bassoon', label: { zh: '巴松', en: 'Bassoon' } },
      { id: 'clarinet', label: { zh: '单簧管', en: 'Clarinet' } },
    ],
  },
  {
    categoryId: 'pipe',
    instruments: [
      { id: 'piccolo', label: { zh: '短笛', en: 'Piccolo' } },
      { id: 'flute', label: { zh: '长笛', en: 'Flute' } },
      { id: 'recorder', label: { zh: '竖笛', en: 'Recorder' } },
      { id: 'pan_flute', label: { zh: '排箫', en: 'Pan Flute' } },
      { id: 'blown_bottle', label: { zh: '瓶口吹奏', en: 'Blown Bottle' } },
      { id: 'shakuhachi', label: { zh: '尺八', en: 'Shakuhachi' } },
      { id: 'whistle', label: { zh: '口哨', en: 'Whistle' } },
      { id: 'ocarina', label: { zh: '陶笛', en: 'Ocarina' } },
    ],
  },
  {
    categoryId: 'synth-lead',
    instruments: [
      { id: 'lead_1_square', label: { zh: '方波主音', en: 'Lead 1 (Square)' } },
      { id: 'lead_2_sawtooth', label: { zh: '锯齿主音', en: 'Lead 2 (Sawtooth)' } },
      { id: 'lead_3_calliope', label: { zh: '汽笛主音', en: 'Lead 3 (Calliope)' } },
      { id: 'lead_4_chiff', label: { zh: '气音主音', en: 'Lead 4 (Chiff)' } },
      { id: 'lead_5_charang', label: { zh: 'Charang 主音', en: 'Lead 5 (Charang)' } },
      { id: 'lead_6_voice', label: { zh: '人声主音', en: 'Lead 6 (Voice)' } },
      { id: 'lead_7_fifths', label: { zh: '五度主音', en: 'Lead 7 (Fifths)' } },
      { id: 'lead_8_bass__lead', label: { zh: '贝斯与主音', en: 'Lead 8 (Bass + Lead)' } },
    ],
  },
  {
    categoryId: 'synth-pad',
    instruments: [
      { id: 'pad_1_new_age', label: { zh: '新世纪铺底', en: 'Pad 1 (New Age)' } },
      { id: 'pad_2_warm', label: { zh: '温暖铺底', en: 'Pad 2 (Warm)' } },
      { id: 'pad_3_polysynth', label: { zh: '多重合成铺底', en: 'Pad 3 (Polysynth)' } },
      { id: 'pad_4_choir', label: { zh: '合唱铺底', en: 'Pad 4 (Choir)' } },
      { id: 'pad_5_bowed', label: { zh: '弓弦铺底', en: 'Pad 5 (Bowed)' } },
      { id: 'pad_6_metallic', label: { zh: '金属铺底', en: 'Pad 6 (Metallic)' } },
      { id: 'pad_7_halo', label: { zh: '光环铺底', en: 'Pad 7 (Halo)' } },
      { id: 'pad_8_sweep', label: { zh: '扫频铺底', en: 'Pad 8 (Sweep)' } },
    ],
  },
  {
    categoryId: 'synth-fx',
    instruments: [
      { id: 'fx_1_rain', label: { zh: '雨声效果', en: 'FX 1 (Rain)' } },
      { id: 'fx_2_soundtrack', label: { zh: '配乐效果', en: 'FX 2 (Soundtrack)' } },
      { id: 'fx_3_crystal', label: { zh: '水晶效果', en: 'FX 3 (Crystal)' } },
      { id: 'fx_4_atmosphere', label: { zh: '氛围效果', en: 'FX 4 (Atmosphere)' } },
      { id: 'fx_5_brightness', label: { zh: '明亮效果', en: 'FX 5 (Brightness)' } },
      { id: 'fx_6_goblins', label: { zh: '妖精效果', en: 'FX 6 (Goblins)' } },
      { id: 'fx_7_echoes', label: { zh: '回声效果', en: 'FX 7 (Echoes)' } },
      { id: 'fx_8_scifi', label: { zh: '科幻效果', en: 'FX 8 (Sci-fi)' } },
    ],
  },
  {
    categoryId: 'ethnic',
    instruments: [
      { id: 'sitar', label: { zh: '西塔琴', en: 'Sitar' } },
      { id: 'banjo', label: { zh: '班卓琴', en: 'Banjo' } },
      { id: 'shamisen', label: { zh: '三味线', en: 'Shamisen' } },
      { id: 'koto', label: { zh: '筝、古筝', en: 'Koto' } },
      { id: 'kalimba', label: { zh: '拇指琴', en: 'Kalimba' } },
      { id: 'bagpipe', label: { zh: '风笛', en: 'Bagpipe' } },
      { id: 'fiddle', label: { zh: '民谣小提琴', en: 'Fiddle' } },
      { id: 'shanai', label: { zh: '唢呐', en: 'Shanai' } },
    ],
  },
  {
    categoryId: 'percussive',
    instruments: [
      { id: 'tinkle_bell', label: { zh: '铃铛', en: 'Tinkle Bell' } },
      { id: 'agogo', label: { zh: '阿哥哥铃', en: 'Agogo' } },
      { id: 'steel_drums', label: { zh: '钢鼓', en: 'Steel Drums' } },
      { id: 'woodblock', label: { zh: '木鱼 / 木块', en: 'Woodblock' } },
      { id: 'taiko_drum', label: { zh: '太鼓', en: 'Taiko Drum' } },
      { id: 'melodic_tom', label: { zh: '旋律嗵鼓', en: 'Melodic Tom' } },
      { id: 'synth_drum', label: { zh: '合成鼓', en: 'Synth Drum' } },
      { id: 'reverse_cymbal', label: { zh: '反转镲', en: 'Reverse Cymbal' } },
    ],
  },
  {
    categoryId: 'sfx',
    instruments: [
      { id: 'guitar_fret_noise', label: { zh: '吉他换把杂音', en: 'Guitar Fret Noise' } },
      { id: 'breath_noise', label: { zh: '呼吸声', en: 'Breath Noise' } },
      { id: 'seashore', label: { zh: '海浪声', en: 'Seashore' } },
      { id: 'bird_tweet', label: { zh: '鸟鸣声', en: 'Bird Tweet' } },
      { id: 'telephone_ring', label: { zh: '电话铃声', en: 'Telephone Ring' } },
      { id: 'helicopter', label: { zh: '直升机', en: 'Helicopter' } },
      { id: 'applause', label: { zh: '掌声', en: 'Applause' } },
      { id: 'gunshot', label: { zh: '枪声', en: 'Gunshot' } },
    ],
  },
];

export const INSTRUMENTS: InstrumentDef[] = [
  {
    id: 'synth-piano',
    label: { zh: '内置合成钢琴', en: 'Built-in Synth Piano' },
    categoryId: 'piano',
    builtIn: true,
  },
  ...GM_INSTRUMENT_GROUPS.flatMap(({ categoryId, instruments }) =>
    instruments.map((instrument) => ({ ...instrument, categoryId })),
  ),
];

// ── Lookup helpers ──────────────────────────────────────────────────

const INSTRUMENT_MAP = new Map(INSTRUMENTS.map((instrument) => [instrument.id, instrument]));

export function getInstrumentDef(id: string): InstrumentDef | undefined {
  return INSTRUMENT_MAP.get(id);
}

export const DEFAULT_INSTRUMENT_ID = 'synth-piano';
