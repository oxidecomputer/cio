let Asciidoctor = require("@asciidoctor/core");
let convert = require("html-to-text").convert;

const asciidoc = Asciidoctor();

const parse = (content) => {
  const doc = asciidoc.load(content);

  const sections = doc
    .getSections()
    .map((section) => formatSection(section, content))
    .reduce((acc, prev) => [...acc, ...prev], []);

  return {
    title: doc
      .getTitle()
      .replace("RFD", "")
      .replace("# ", "")
      .replace("= ", "")
      .trim()
      .split(' ')
      .slice(1)
      .join(' '),
    sections
  };
};

const formatSection = (section, content) => {
  const formattedSections = [];
  for (const s of section.getSections()) {
    formattedSections.push(...formatSection(s, content));
  }
  const parentSections = [];
  let level = section.getLevel() - 1;
  let currSection = section.getParent();

  while (level-- && currSection) {
    parentSections.push(currSection.getName());
    currSection = currSection.getParent();
  }

  return [
    {
      section_id: section.getId(),
      name: section.getName(),
      content: convert(
        section
          .getBlocks()
          .filter((block) => block.context !== "section")
          .map((block) => block.convert())
          .join("")
      ),
      parents: parentSections
    },
    ...formattedSections,
  ];
};

let content = require("fs").readFileSync(0, 'utf-8');
console.log(JSON.stringify(parse(content)))