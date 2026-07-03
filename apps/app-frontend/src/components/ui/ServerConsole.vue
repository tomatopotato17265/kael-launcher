<script setup lang="ts">
import { BaseTerminal, injectNotificationManager } from '@kael/ui'
import { onBeforeUnmount, ref, watch } from 'vue'

import { getLogs, sendCommand } from '@/helpers/hosting'

const props = defineProps<{
	serverId: string
	running: boolean
}>()

const { handleError } = injectNotificationManager()

const terminal = ref<InstanceType<typeof BaseTerminal> | null>(null)

let rendered = 0
let timer: ReturnType<typeof setInterval> | undefined

async function pump() {
	try {
		const logs = await getLogs(props.serverId)
		if (!terminal.value) return
		if (logs.length < rendered) {
			terminal.value.reset()
			rendered = 0
		}
		for (const line of logs.slice(rendered)) {
			terminal.value.writeln(line)
		}
		rendered = logs.length
	} catch {
		// The server may have just stopped; the next poll or teardown handles it
	}
}

function startPolling() {
	stopPolling()
	void pump()
	timer = setInterval(() => void pump(), 1000)
}

function stopPolling() {
	if (timer) {
		clearInterval(timer)
		timer = undefined
	}
}

watch(
	() => props.running,
	(running) => {
		if (running) startPolling()
		else stopPolling()
	},
	{ immediate: true },
)

function onCommand(command: string) {
	sendCommand(props.serverId, command).then(pump).catch(handleError)
}

onBeforeUnmount(stopPolling)
</script>

<template>
	<div class="server-console">
		<BaseTerminal
			ref="terminal"
			show-input
			:disable-input="!running"
			empty-state-type="server"
			@command="onCommand"
		/>
	</div>
</template>

<style scoped>
.server-console {
	height: 24rem;
	width: 100%;
}
</style>
