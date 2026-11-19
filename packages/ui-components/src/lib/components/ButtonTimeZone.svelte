<script lang="ts">
	import { ClockOutline } from 'flowbite-svelte-icons';
	import { useLocalTime as useLocalTimeStore } from '../storesGeneric/useLocalTime';
	import type { Writable } from 'svelte/store';

	/**
	 * Toggle for displaying timestamps in the browser's local timezone vs UTC.
	 * Sits next to the dark-mode / scrub buttons in the sidebar.
	 *
	 * The store is injectable for testing; it defaults to the global preference store.
	 */
	export let useLocalTime: Writable<boolean> = useLocalTimeStore;

	function toggle() {
		useLocalTime.update((value) => !value);
	}
</script>

<button
	type="button"
	on:click={toggle}
	data-testid="timezone-toggle"
	aria-pressed={$useLocalTime}
	title={$useLocalTime
		? 'Showing local time — click for UTC'
		: 'Showing UTC — click for local time'}
	aria-label={$useLocalTime
		? 'Switch timestamps to UTC'
		: 'Switch timestamps to local time'}
	class="inline-flex items-center gap-1 rounded-lg px-2 py-1.5 text-xs font-semibold tabular-nums text-gray-500 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-700 dark:hover:text-white"
>
	<ClockOutline class="h-4 w-4 shrink-0" />
	<span>{$useLocalTime ? 'Local' : 'UTC'}</span>
</button>
