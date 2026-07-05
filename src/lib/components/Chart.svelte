<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import * as echarts from 'echarts';

  let { options = {}, height = 240, class: className = '' }: {
    options: Record<string, unknown>;
    height?: number;
    class?: string;
  } = $props();

  let chartEl: HTMLDivElement;
  let chart: echarts.ECharts | null = null;

  onMount(() => {
    chart = echarts.init(chartEl);
    chart.setOption(options);
    const ro = new ResizeObserver(() => chart?.resize());
    ro.observe(chartEl);
    return () => { ro.disconnect(); chart?.dispose(); };
  });

  $effect(() => {
    if (chart) chart.setOption(options, true);
  });
</script>

<div bind:this={chartEl} class={className} style="height: {height}px; width: 100%;"></div>
