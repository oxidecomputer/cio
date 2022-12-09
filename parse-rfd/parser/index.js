let Asciidoctor = require("@asciidoctor/core");
let crypto = require("crypto");
let convert = require("html-to-text").convert;

const asciidoc = Asciidoctor();

const parse = (content) => {
  const doc = asciidoc.load(content);

  const sections = doc
    .getSections()
    .map((section) => formatSection(section, content))
    .reduce((acc, prev) => [...acc, ...prev], []);

  return sections;
};
const formatSection = (section, content) => {
  const formattedSections = [];
  for (const s of section.getSections()) {
    formattedSections.push(...formatSection(s, content));
  }
  const sectionNames = [section.getName()];
  let level = section.getLevel() - 1;
  let currSection = section.getParent();

  while (level-- && currSection) {
    sectionNames.push((currSection).getName());
    currSection = currSection.getParent();
  }

  level = section.getLevel();
  isValidLevel(level);

  return [
    {
      section_id: section.getId(),
      anchor: section.getId(),
      name: section.getName(),
      level,
      content: convert(
        section
          .getBlocks()
          .filter((block) => block.context !== "section")
          .map((block) => block.convert())
          .join("")
      ),
      ...formatLevel(level, sectionNames),
    },
    ...formattedSections,
  ];
};

/*
 * Level Formatting
 */

function isValidLevel(input) {
  if (typeof input !== "number" || input < 0 || input > 6) {
    throw new Error("Invalid level");
  }
}

const RADIO = "hierarchy_radio_lvl";
const LEVEL = "hierarchy_lvl";

const RADIO_0 = `${RADIO}0`;
const RADIO_1 = `${RADIO}1`;
const RADIO_2 = `${RADIO}2`;
const RADIO_3 = `${RADIO}3`;
const RADIO_4 = `${RADIO}4`;
const RADIO_5 = `${RADIO}5`;

const LEVEL_0 = `${LEVEL}0`;
const LEVEL_1 = `${LEVEL}1`;
const LEVEL_2 = `${LEVEL}2`;
const LEVEL_3 = `${LEVEL}3`;
const LEVEL_4 = `${LEVEL}4`;
const LEVEL_5 = `${LEVEL}5`;
const LEVEL_6 = `${LEVEL}6`;

const formatLevel = (level, sections) => {
  switch (level) {
    case 0:
      return {
        [LEVEL_0]: sections[0],
        [RADIO_0]: sections[0],
      };
    case 1:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [RADIO_1]: sections[1],
      };
    case 2:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [LEVEL_2]: sections[2],
        [RADIO_2]: sections[2],
      };
    case 3:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [LEVEL_2]: sections[2],
        [LEVEL_3]: sections[3],
        [RADIO_3]: sections[3],
      };
    case 4:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [LEVEL_2]: sections[2],
        [LEVEL_3]: sections[3],
        [LEVEL_4]: sections[4],
        [RADIO_4]: sections[4],
      };
    case 5:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [LEVEL_2]: sections[2],
        [LEVEL_3]: sections[3],
        [LEVEL_4]: sections[4],
        [LEVEL_5]: sections[5],
        [RADIO_5]: sections[5],
      };
    case 6:
      return {
        [LEVEL_0]: sections[0],
        [LEVEL_1]: sections[1],
        [LEVEL_2]: sections[2],
        [LEVEL_3]: sections[3],
        [LEVEL_4]: sections[4],
        [LEVEL_5]: sections[5],
        [LEVEL_6]: sections[6],
        [RADIO_5]: sections[5], // This is intentionally 5 as lvl 6 isn't selectable
      };
  }
};

let content = require("fs").readFileSync(0, 'utf-8');
console.log(content)