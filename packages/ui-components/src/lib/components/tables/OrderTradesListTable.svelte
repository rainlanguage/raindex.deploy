<script lang="ts">
  import { createInfiniteQuery } from "@tanstack/svelte-query";
  import TanstackAppTable from "../TanstackAppTable.svelte";
  import { QKEY_ORDER_TRADES_LIST } from "../../queries/keys";
  import { TableBodyCell, TableHeadCell } from "flowbite-svelte";
  import { formatTimestampSecondsAsLocal } from "../../services/time";
  import { useLocalTime } from "../../storesGeneric/useLocalTime";
  import Hash, { HashType } from "../Hash.svelte";
  import { BugOutline } from "flowbite-svelte-icons";
  import type {
    RaindexOrder,
    RaindexTrade,
    RaindexTradesListResult,
  } from "@rainlanguage/raindex";
  import TableTimeFilters from "../charts/TableTimeFilters.svelte";
  import Tooltip from "../Tooltip.svelte";

  export let order: RaindexOrder;
  export let rpcs: string[] | undefined = undefined;
  export let handleDebugTradeModal:
    | ((hash: string, rpcs: string[]) => void)
    | undefined = undefined;

  let startTimestamp: number | undefined;
  let endTimestamp: number | undefined;
  let tradesCount: number | undefined;

  $: orderTradesQuery = createInfiniteQuery({
    queryKey: [order.id, QKEY_ORDER_TRADES_LIST + order.id],
    queryFn: async ({ pageParam }: { pageParam: number }) => {
      tradesCount = undefined;

      const result = await order.getTradesList(
        startTimestamp ? BigInt(startTimestamp) : undefined,
        endTimestamp ? BigInt(endTimestamp) : undefined,
        pageParam + 1,
      );
      if (result.error) throw new Error(result.error.readableMsg);

      tradesCount = Number(result.value.totalCount);

      return result.value;
    },
    initialPageParam: 0,
    getNextPageParam: (
      lastPage: RaindexTradesListResult,
      allPages: RaindexTradesListResult[],
      lastPageParam: number,
    ) => {
      const fetchedCount = allPages.reduce(
        (sum, p) => sum + p.trades.length,
        0,
      );
      return fetchedCount < Number(lastPage.totalCount)
        ? lastPageParam + 1
        : undefined;
    },
  });

  const AppTable = TanstackAppTable<RaindexTrade, RaindexTradesListResult>;
</script>

<AppTable
  query={orderTradesQuery}
  dataSelector={(page) => page.trades}
  emptyMessage="No trades found"
  rowHoverable={false}
  queryKey={order.id}
>
  <svelte:fragment slot="info">
    {#if tradesCount !== undefined}
      <div class="px-2">{tradesCount} Trades</div>
    {/if}
  </svelte:fragment>
  <svelte:fragment slot="timeFilter">
    <TableTimeFilters bind:startTimestamp bind:endTimestamp />
  </svelte:fragment>
  <svelte:fragment slot="head">
    <TableHeadCell padding="p-4" class="w-[15%]">Date</TableHeadCell>
    <TableHeadCell padding="p-4" class="w-[20%]">Transaction</TableHeadCell>
    <TableHeadCell padding="p-2" class="w-[18%]">Input</TableHeadCell>
    <TableHeadCell padding="p-2" class="w-[18%]">Output</TableHeadCell>
    <TableHeadCell padding="p-2" class="w-[25%]">IO Ratio</TableHeadCell>
    <TableHeadCell padding="p-0" class="w-[4%]"
      ><span class="sr-only">Actions</span></TableHeadCell
    >
  </svelte:fragment>

  <svelte:fragment slot="bodyRow" let:item>
    {@const inputAmt = Number(item.inputVaultBalanceChange.formattedAmount)}
    {@const outputAmt = Number(item.outputVaultBalanceChange.formattedAmount)}
    {@const ioRatio = Math.abs(inputAmt / outputAmt)}
    {@const oiRatio = Math.abs(outputAmt / inputAmt)}
    {@const validRatio = Number.isFinite(ioRatio) && Number.isFinite(oiRatio)}
    <TableBodyCell tdClass="px-4 py-2">
      {formatTimestampSecondsAsLocal(BigInt(item.timestamp), $useLocalTime)}
    </TableBodyCell>
    <TableBodyCell tdClass="px-4 py-2">
      <div class="flex flex-col gap-1 text-sm">
        <div class="flex items-center gap-1">
          <span class="text-gray-500 dark:text-gray-400">Sender:</span>
          <Hash type={HashType.Wallet} value={item.transaction.from} />
        </div>
        <div class="flex items-center gap-1">
          <span class="text-gray-500 dark:text-gray-400">Tx:</span>
          <Hash type={HashType.Transaction} value={item.transaction.id} />
        </div>
      </div>
    </TableBodyCell>
    <TableBodyCell tdClass="p-2" data-testid="input">
      <div class="flex flex-col overflow-hidden">
        <span class="truncate font-medium"
          >{item.inputVaultBalanceChange.token.symbol}</span
        >
        <span
          id={`input-${item.id}`}
          class="truncate text-sm text-gray-500 dark:text-gray-400"
          >{item.inputVaultBalanceChange.formattedAmount}</span
        >
        <Tooltip triggeredBy={`#input-${item.id}`}>
          {item.inputVaultBalanceChange.formattedAmount}
          {item.inputVaultBalanceChange.token.symbol}
        </Tooltip>
      </div>
    </TableBodyCell>
    <TableBodyCell tdClass="p-2" data-testid="output">
      <div class="flex flex-col overflow-hidden">
        <span class="truncate font-medium"
          >{item.outputVaultBalanceChange.token.symbol}</span
        >
        <span
          id={`output-${item.id}`}
          class="truncate text-sm text-gray-500 dark:text-gray-400"
          >{item.outputVaultBalanceChange.formattedAmount}</span
        >
        <Tooltip triggeredBy={`#output-${item.id}`}>
          {item.outputVaultBalanceChange.formattedAmount}
          {item.outputVaultBalanceChange.token.symbol}
        </Tooltip>
      </div>
    </TableBodyCell>
    <TableBodyCell tdClass="p-2" data-testid="io-ratio">
      <div id={`io-ratio-${item.id}`} class="truncate">
        {#if validRatio}
          {ioRatio}
          <span class="text-gray-400">({oiRatio})</span>
        {:else}
          -
        {/if}
      </div>
      <Tooltip triggeredBy={`#io-ratio-${item.id}`}>
        {#if validRatio}
          {ioRatio} ({oiRatio})
        {:else}
          -
        {/if}
      </Tooltip>
    </TableBodyCell>
    <TableBodyCell tdClass="py-2">
      {#if rpcs && handleDebugTradeModal}
        <button
          data-testid="debug-trade-button"
          class="text-gray-500 hover:text-gray-700"
          title="Debug trade"
          aria-label="Debug trade"
          on:click={() => {
            if (rpcs) handleDebugTradeModal(item.transaction.id, rpcs);
          }}
        >
          <BugOutline size="xs" />
        </button>
      {/if}
    </TableBodyCell>
  </svelte:fragment>
</AppTable>
