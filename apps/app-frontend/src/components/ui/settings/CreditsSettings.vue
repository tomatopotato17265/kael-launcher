<script setup lang="ts">
import { ExternalIcon } from '@kael/assets'
import { defineMessages, useVIntl } from '@kael/ui'

const { formatMessage } = useVIntl()

const messages = defineMessages({
	skinEditorTitle: {
		id: 'app.credits-settings.skin-editor.title',
		defaultMessage: 'Skin Editor',
	},
	skinEditorDescription: {
		id: 'app.credits-settings.skin-editor.description',
		defaultMessage: 'The in-app Minecraft skin editor, forked from Blockbench.',
	},
	blockbenchLink: {
		id: 'app.credits-settings.skin-editor.blockbench-link',
		defaultMessage: 'Blockbench',
	},
})
</script>
<template>
	<div>
		<div class="mt-6 flex items-center justify-between">
			<div>
				<h2 class="m-0 text-lg font-semibold text-contrast">
					{{ formatMessage(messages.skinEditorTitle) }}
				</h2>
				<p class="m-0 mt-1">{{ formatMessage(messages.skinEditorDescription) }}</p>
			</div>
			<a
				href="https://www.blockbench.net"
				target="_blank"
				rel="noopener noreferrer"
				class="flex shrink-0 items-center gap-1.5 text-brand hover:underline [&>svg]:size-4"
			>
				{{ formatMessage(messages.blockbenchLink) }}
				<ExternalIcon />
			</a>
		</div>
	</div>
</template>
