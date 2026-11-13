import { expect, test } from '@playwright/test';
import { existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

// Schemas in other formats than JSON Schema written as JSON: the built-in
// JSON Schema written in YAML, and a format registered from JavaScript
// (`registerSchemaFormat`) with the files it refers to and its options, in
// the tools (`compileSchema`, `schemaImports`, `validate`, `convert`,
// `schemaWarnings`), a `ConfigEditor` and a `ConfigDocuments` view. Build
// the packages first with `npm run build` in js/.

const js = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'js');
const built = existsSync(join(js, 'config-editor', 'pkg', 'config_editor_bg.wasm'));
const pageUrl = (name) => pathToFileURL(join(js, 'examples', name)).href;

test.skip(!built, 'js packages not built: run `npm run build` in js/');

// `name: type` lines; `use: <file>` takes a resource's lines; `lose: <what>`
// reports a loss on its line; the options' `required` lists required fields.
const FIELDS = `({
  name: "Fields",
  label: "Fields schema",
  extensions: ["fields", "schema.fields"],
  compile(text, resources, options) {
    const properties = {};
    const losses = [];
    const read = (source, location) => source.split("\\n").forEach((line, index) => {
      const [key, value] = line.split(":").map((part) => part.trim());
      if (!key) return;
      if (key === "use") {
        if (!(value in resources)) throw new Error("no resource " + value);
        read(resources[value], value);
      } else if (key === "lose") {
        losses.push({ message: value + " is not checked", construct: value, effect: "loosened", line: index + 1, column: 1, ...(location ? { location } : {}) });
      } else {
        properties[key] = { type: value };
      }
    });
    read(text);
    return { schema: { properties, required: options.required ?? [] }, losses };
  },
  imports(text) {
    return text.split("\\n").filter((line) => line.startsWith("use:")).map((line) => line.slice(4).trim());
  },
})`;

async function open(page) {
  page.errors = [];
  page.on('pageerror', (error) => page.errors.push(String(error)));
  await page.goto(pageUrl('vanilla.html'));
  await expect(page.locator('body')).toHaveAttribute('data-ready', 'true', { timeout: 20000 });
  await page.evaluate(async (fields) => {
    window.core = await import('../config-editor/index.js');
    window.tools = await import('../config-editor/tools.js');
    // eslint-disable-next-line no-new-func
    window.fieldsName = window.core.registerSchemaFormat(new Function(`return ${fields}`)());
  }, FIELDS);
}

/** What `run` (a function of `core`, `tools`) returns, or the message it throws. */
const attempt = (page, run) =>
  page.evaluate((source) => {
    try {
      // eslint-disable-next-line no-new-func
      return new Function('core', 'tools', `return (${source})(core, tools)`)(window.core, window.tools);
    } catch (error) {
      return { threw: String(error.message ?? error) };
    }
  }, run.toString());

test.describe('schema formats in the tools', () => {
  test.beforeEach(async ({ page }) => open(page));

  test('a registered format compiles with its resources and options, and reports its losses', async ({ page }) => {
    const compiled = await attempt(page, (core, tools) =>
      tools.compileSchema('port: integer\nuse: common.fields\nlose: order\n', {
        format: 'fields',
        resources: { 'common.fields': 'name: string\nlose: unique\n' },
        options: { required: ['port'] },
      }),
    );
    expect(JSON.parse(compiled.schema)).toEqual({
      properties: { port: { type: 'integer' }, name: { type: 'string' } },
      required: ['port'],
    });
    expect(compiled.losses).toEqual([
      { message: 'unique is not checked', construct: 'unique', effect: 'loosened', location: 'common.fields', line: 2, column: 1 },
      { message: 'order is not checked', construct: 'order', effect: 'loosened', line: 3, column: 1 },
    ]);
    expect(await attempt(page, (core, tools) => tools.schemaImports('use: a.fields\nuse: b.fields\nuse: a.fields\n', 'fields'))).toEqual([
      'a.fields',
      'b.fields',
    ]);
    expect(await attempt(page, (core, tools) => tools.schemaImports('{}'))).toEqual([]);
  });

  test('validate, convert and schemaWarnings take the format, resources and options', async ({ page }) => {
    const result = await attempt(page, (core, tools) => {
      const source = { schemaFormat: 'fields', schemaResources: { 'common.fields': 'port: integer\n' }, schemaOptions: { required: ['name'] } };
      return {
        problems: tools.validate('yaml', 'port: x\n', 'use: common.fields\nlose: order\n', source).map((problem) => problem.message),
        warnings: tools.schemaWarnings('use: common.fields\nlose: order\n', source),
        converted: tools.convert('xml', '<port>80</port>', 'json', 'use: common.fields\n', source).text,
        yaml: tools.validate('json', '{"listen": "x"}', 'properties:\n  listen: {type: integer}\n', { schemaFormat: 'json-schema-yaml' }).map((problem) => problem.message),
      };
    });
    expect(result).toEqual({
      problems: ['"name" is a required property', '/port: "x" is not of type "integer"'],
      warnings: ['2:1: order is not checked'],
      converted: '{\n  "port": 80\n}\n',
      yaml: ['/listen: "x" is not of type "integer"'],
    });
  });

  test('a format is named, never guessed; errors say which format refused what', async ({ page }) => {
    const result = await attempt(page, (core) => ({
      name: window.fieldsName,
      formats: core.schemaFormats().map((format) => [format.name, format.label, format.extensions]),
      files: ['a/app.schema.yaml', 'crd.yml', 'app.json', 'app.schema.fields', 'x.fields', 'x.xsdx', 'fields', 'pom.xsd'].map((path) => core.schemaFormatForFile(path)),
    }));
    expect(result.name).toBe('fields');
    expect(result.formats).toEqual([
      ['json-schema', 'JSON Schema', ['json', 'schema.json']],
      ['json-schema-yaml', 'JSON Schema (YAML)', ['yaml', 'yml', 'schema.yaml', 'schema.yml']],
      ['openapi', 'OpenAPI', ['openapi.json', 'openapi.yaml', 'openapi.yml', 'swagger.json', 'swagger.yaml']],
      ['xsd', 'XSD', ['xsd']],
      ['fields', 'Fields schema', ['fields', 'schema.fields']],
    ]);
    expect(result.files).toEqual(['json-schema-yaml', 'json-schema-yaml', 'json-schema', 'fields', 'fields', null, null, 'xsd']);
    const errors = await attempt(page, (core, tools) => {
      const message = (run) => {
        try {
          run();
          return 'no error';
        } catch (error) {
          return error.message;
        }
      };
      return [
        message(() => tools.compileSchema('use: gone.fields\n', { format: 'fields' })),
        message(() => tools.compileSchema('properties: [\n', { format: 'json-schema-yaml' })).replace(/ \(line.*/, ''),
        message(() => tools.compileSchema('{}', { format: 'jtd' })),
        message(() => core.registerSchemaFormat({ name: 'json-schema', compile: () => ({ schema: {} }) })),
        message(() => core.registerSchemaFormat({ name: 'nothing' })),
        message(() => core.registerSchemaFormat({ name: 'bad', compile: () => 5 }) && tools.compileSchema('', { format: 'bad' })),
        message(() => core.registerSchemaFormat({ name: 'typo', compile: () => ({ schema: { type: 5 } }) }) && tools.compileSchema('', { format: 'typo' })),
      ];
    });
    expect(errors[0]).toBe('Invalid Fields schema: no resource gone.fields');
    expect(errors[1]).toMatch(/^Invalid JSON Schema \(YAML\): /);
    expect(errors[2]).toBe('unknown schema format "jtd" (expected json-schema, json-schema-yaml, openapi, xsd); registered: fields');
    expect(errors[3]).toBe('"json-schema" is a built-in schema format');
    expect(errors[4]).toBe('a schema format needs compile(text, resources, options)');
    expect(errors[5]).toBe("Invalid bad: bad's compile() must return { schema, losses? }");
    expect(errors[6]).toMatch(/^Invalid JSON Schema: /);
  });

  test('registering a name again replaces its compiler, for a schema already compiled too', async ({ page }) => {
    const result = await attempt(page, (core, tools) => {
      core.registerSchemaFormat({ name: 'Swap', compile: () => ({ schema: { type: 'string' } }) });
      const before = tools.validate('json', '1', 'x', { schemaFormat: 'swap' }).length;
      core.registerSchemaFormat({ name: 'swap', compile: () => ({ schema: { type: 'integer' } }) });
      // The same text: the schema kept from the call before is not reused.
      const after = tools.validate('json', '1', 'x', { schemaFormat: 'swap' }).length;
      return { before, after };
    });
    expect(result).toEqual({ before: 1, after: 0 });
  });
});

test.describe('XML Schema', () => {
  test.beforeEach(async ({ page }) => open(page));

  const XSD = `<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">
  <xs:element name="server">
    <xs:annotation><xs:documentation>The server.</xs:documentation></xs:annotation>
    <xs:complexType>
      <xs:sequence>
        <xs:element name="host" type="xs:string"/>
        <xs:element name="price" type="xs:decimal" minOccurs="0"/>
      </xs:sequence>
      <xs:attribute name="port" type="xs:unsignedShort" use="required"/>
    </xs:complexType>
    <xs:key name="k"><xs:selector xpath="host"/><xs:field xpath="."/></xs:key>
  </xs:element>
</xs:schema>`;

  test('an XSD compiles, types and validates every view, and reports what it cannot check', async ({ page }) => {
    const result = await page.evaluate(async (xsd) => {
      const source = { schemaFormat: 'xsd' };
      const { schema, losses } = window.tools.compileSchema(xsd, { format: 'xsd', options: { root: 'server' } });
      const problems = (language, text) => window.tools.validate(language, text, xsd, source).map((problem) => problem.message);
      return {
        description: JSON.parse(schema).$defs['element.server'].description,
        losses: losses.map(({ construct, effect, line }) => ({ construct, effect, line })),
        xml: problems('xml', '<server port="80"><host>h</host><price>1.50</price></server>'),
        badXml: problems('xml', '<server port="99999"><hots>h</hots></server>'),
        yaml: problems('yaml', 'server:\n  port: 80\n  host: h\n  price: x\n'),
        converted: window.tools.convert('yaml', 'server:\n  port: 80\n  host: h\n', 'xml', xsd, source).text,
      };
    }, XSD);
    expect(result.description).toBe('The server.');
    expect(result.losses).toEqual([
      { construct: 'xs:key', effect: 'not-checked', line: 11 },
      { construct: 'xs:sequence order', effect: 'not-checked', line: 5 },
    ]);
    expect(result.xml).toEqual([]);
    expect(result.badXml.sort()).toEqual([
      '/server/port: 99999 is greater than the maximum of 65535',
      '/server: "host" is a required property',
      '/server: "hots" is not allowed here',
    ]);
    expect(result.yaml).toEqual(['/server/price: "x" is not of type "number"']);
    expect(result.converted).toBe('<server port="80">\n  <host>h</host>\n</server>\n');
  });
});

test.describe('schemas in several files', () => {
  test.beforeEach(async ({ page }) => open(page));

  test('an XSD include and a JSON $ref read their files from the resources', async ({ page }) => {
    const result = await attempt(page, (core, tools) => {
      const xs = 'xmlns:xs="http://www.w3.org/2001/XMLSchema"';
      const main = `<xs:schema ${xs}><xs:include schemaLocation="common.xsd"/><xs:element name="port" type="portType"/></xs:schema>`;
      const common = `<xs:schema ${xs}><xs:simpleType name="portType"><xs:restriction base="xs:int"><xs:maxInclusive value="65535"/></xs:restriction></xs:simpleType></xs:schema>`;
      const json = '{"properties":{"port":{"$ref":"defs.json#/port"}}}';
      const defs = '{"port":{"type":"integer","maximum":10}}';
      let missing;
      try {
        tools.compileSchema(main, { format: 'xsd' });
      } catch (error) {
        missing = error.message;
      }
      return {
        imports: tools.schemaImports(main, 'xsd'),
        jsonImports: tools.schemaImports(json),
        missing,
        xsd: tools.validate('xml', '<port>70000</port>', main, { schemaFormat: 'xsd', schemaResources: { 'common.xsd': common } }).map((p) => p.message),
        json: tools.validate('json', '{"port": 11}', json, { schemaResources: { 'defs.json': defs } }).map((p) => p.message),
      };
    });
    expect(result.imports).toEqual(['common.xsd']);
    expect(result.jsonImports).toEqual(['defs.json']);
    expect(result.missing).toContain('"common.xsd" is not among the schema\'s resources');
    expect(result.xsd).toEqual(['/port: 70000 is greater than the maximum of 65535']);
    expect(result.json).toEqual(['/port: 11 is greater than the maximum of 10']);
  });
});

test.describe('OpenAPI', () => {
  test.beforeEach(async ({ page }) => open(page));

  test('a named schema of an OpenAPI document validates a config', async ({ page }) => {
    const result = await attempt(page, (core, tools) => {
      const api = JSON.stringify({
        openapi: '3.0.3',
        components: { schemas: {
          Server: { type: 'object', properties: { port: { type: 'integer', nullable: true }, tls: { $ref: '#/components/schemas/Tls' } } },
          Tls: { type: 'object', properties: { cert: { type: 'string' } }, additionalProperties: false },
        } },
      });
      const source = { schemaFormat: 'openapi', schemaOptions: { schema: 'Server' } };
      return {
        valid: tools.validate('yaml', 'port: null\ntls: {cert: c.pem}\n', api, source).length,
        invalid: tools.validate('yaml', 'port: x\ntls: {key: k}\n', api, source).map((p) => p.message).sort(),
        file: core.schemaFormatForFile('petstore.openapi.yaml'),
      };
    });
    expect(result.valid).toBe(0);
    expect(result.invalid).toEqual(['/port: "x" is not of type "null", "integer"', '/tls: "key" is not allowed here']);
    expect(result.file).toBe('openapi');
  });
});

test.describe('a custom language whose values are text', () => {
  test.beforeEach(async ({ page }) => open(page));

  test('with scalars "text", a value reads as the type the schema gives its key', async ({ page }) => {
    const result = await attempt(page, (core, tools) => {
      const definition = (label, extra) => ({
        label,
        separators: ['='],
        parse: (text) => Object.fromEntries(text.split('\n').filter(Boolean).map((line) => line.split('='))),
        serialize: (value) => Object.entries(value).map(([key, item]) => key + '=' + item).join('\n') + '\n',
        ...extra,
      });
      const text = core.registerLanguage(definition('Lines', { scalars: 'text' }));
      const typed = core.registerLanguage(definition('Plain', {}));
      const schema = JSON.stringify({ properties: { port: { type: 'integer' }, debug: { type: 'boolean' }, name: { type: 'string' } } });
      const problems = (language, source) => tools.validate(language, source, schema).map((problem) => problem.message);
      let refused;
      try {
        core.registerLanguage(definition('Bad', { scalars: 'numbers' }));
      } catch (error) {
        refused = error.message;
      }
      return {
        text: problems(text, 'port=80\ndebug=true\nname=007\n'),
        wrong: problems(text, 'port=eighty\n'),
        typed: problems(typed, 'port=80\n'),
        converted: tools.convert(text, 'port=80\nname=007\n', 'json', schema).text,
        refused,
      };
    });
    expect(result).toEqual({
      text: [],
      wrong: ['/port: "eighty" is not of type "integer"'],
      typed: ['/port: "80" is not of type "integer"'],
      converted: '{\n  "port": 80,\n  "name": "007"\n}\n',
      refused: 'scalars must be "typed" or "text"',
    });
  });
});

test.describe('schema formats in the editor and the documents view', () => {
  test.beforeEach(async ({ page }) => open(page));

  test('an editor compiles its schema in its format and reports the losses as schema warnings', async ({ page }) => {
    const result = await page.evaluate(async () => {
      window.editor.destroy();
      const warned = [];
      const heard = [];
      window.editor = new window.core.ConfigEditor(document.getElementById('editor'), {
        language: 'yaml',
        value: 'port: x\n',
        schema: 'use: common.fields\nlose: order\n',
        schemaFormat: 'fields',
        schemaResources: { 'common.fields': 'port: integer\n' },
        schemaOptions: { required: ['port'] },
        onSchemaWarning: (message) => warned.push(message),
        onProblems: (problems) => heard.push(problems.map((problem) => problem.message)),
      });
      await new Promise((resolve) => setTimeout(resolve, 600));
      const refused = (() => {
        try {
          window.editor.update({ schemaFormat: 'json-schema' });
          return 'no error';
        } catch (error) {
          return error.message;
        }
      })();
      const invalid = (() => {
        try {
          new window.core.ConfigEditor(document.createElement('div'), { schema: 'use: gone\n', schemaFormat: 'fields' });
          return 'no error';
        } catch (error) {
          return error.message;
        }
      })();
      return { warned, heard: heard.at(-1), refused, invalid };
    });
    expect(result.warned).toEqual(['2:1: order is not checked']);
    expect(result.heard).toEqual(['/port: "x" is not of type "integer"']);
    expect(result.refused).toContain('schemaFormat');
    expect(result.invalid).toBe('Invalid Fields schema: no resource gone');
  });

  test('documents compile their schema in their format, and share it when it is the same', async ({ page }) => {
    const result = await page.evaluate(async () => {
      window.editor.destroy();
      const heard = {};
      const doc = (id, value, extra) => ({ id, fileName: `${id}.yaml`, value, schema: 'port: integer\n', schemaFormat: 'fields', ...extra });
      const host = document.getElementById('editor');
      const view = new window.core.ConfigDocuments(host, {
        documents: [doc('a', 'port: x\n'), doc('b', 'port: 1\n', { schemaOptions: { required: ['name'] } })],
        onProblems: (id, problems) => (heard[id] = problems.map((problem) => problem.message)),
      });
      await new Promise((resolve) => setTimeout(resolve, 600));
      view.select('b');
      await new Promise((resolve) => setTimeout(resolve, 600));
      const thrown = (() => {
        try {
          view.update({ documents: [doc('a', 'port: x\n', { schemaFormat: 'jtd' })] });
          return 'no error';
        } catch (error) {
          return error.message;
        }
      })();
      view.destroy();
      return { heard, thrown };
    });
    expect(result.heard).toEqual({ a: ['/port: "x" is not of type "integer"'], b: ['"name" is a required property'] });
    expect(result.thrown).toMatch(/^unknown schema format "jtd"/);
  });
});
