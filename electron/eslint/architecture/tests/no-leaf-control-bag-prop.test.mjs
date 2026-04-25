import { ruleTester } from '../../test-setup.mjs';
import rule from '../rules/no-leaf-control-bag-prop.mjs';

ruleTester.run('architecture/no-leaf-control-bag-prop', rule, {
  valid: [
    {
      code: `
        function Parent(){
          const reader = useScoutReader();
          return <ScoutActForm itemId={reader.item.id} onClose={reader.act.close} />;
        }
      `,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
    },
    {
      code: `
        function Parent(){
          const editor = useProjectEditor();
          return (
            <>
              <ProjectEditorFields
                name={editor.fields.name}
                onName={editor.fields.setName}
                githubRepo={editor.fields.githubRepo}
                onGithubRepo={editor.fields.setGithubRepo}
              />
              <button disabled={!editor.fields.name.trim()}>Save</button>
            </>
          );
        }
      `,
      filename: 'src/renderer/domains/settings/ui/Parent.tsx',
    },
    {
      code: `
        function Parent(){
          const form = useSettingsAccounts();
          return <AddTaskFormBody title={form.draft.title} setTitle={form.draft.setTitle} />;
        }
      `,
      filename: 'src/renderer/domains/captain/ui/Parent.tsx',
    },
  ],
  invalid: [
    {
      code: `
        function Parent(){
          const reader = useScoutReader();
          return (
            <ScoutActForm
              projects={reader.act.projects}
              actProject={reader.act.project}
              setActProject={reader.act.setProject}
              actPrompt={reader.act.prompt}
              setActPrompt={reader.act.setPrompt}
              acting={reader.act.pending}
              actResult={reader.act.result}
              onAct={reader.act.handle}
            />
          );
        }
      `,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
      errors: [{ messageId: 'leafBag' }],
    },
    {
      code: `
        function Parent(){
          const accounts = useSettingsAccounts();
          return (
            <AddCredentialForm
              setupToken={accounts.form.setupToken}
              setupLabel={accounts.form.setupLabel}
              isPending={accounts.mutations.addTokenMut.isPending}
              onTokenChange={accounts.form.setSetupToken}
              onLabelChange={accounts.form.setSetupLabel}
              onAdd={accounts.actions.handleAdd}
              onCancel={accounts.actions.handleCancel}
            />
          );
        }
      `,
      filename: 'src/renderer/domains/settings/ui/Parent.tsx',
      errors: [{ messageId: 'leafBag' }],
    },
    {
      code: `
        function Parent(){
          const composer = useMarkdownImageQAChat();
          return (
            <Frame
              composer={
                <ImageQAComposer
                  question={composer.text.question}
                  textareaRef={composer.text.textareaRef}
                  handleChange={composer.text.handleChange}
                  doSubmit={composer.submit.doSubmit}
                  pending={composer.submit.pending}
                  image={composer.image.image}
                  preview={composer.image.preview}
                  setImageFile={composer.image.setImageFile}
                  removeImage={composer.image.removeImage}
                  fileRef={composer.image.fileRef}
                />
              }
            />
          );
        }
      `,
      filename: 'src/renderer/domains/scout/ui/Parent.tsx',
      errors: [{ messageId: 'leafBag' }],
    },
  ],
});
