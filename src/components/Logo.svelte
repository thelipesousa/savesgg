<script lang="ts">
  // Coração pixel da logo (igual a design/marca/savesgg-coracao.svg e ao ícone da bandeja).
  interface Props {
    tamanho?: number;
  }
  let { tamanho = 28 }: Props = $props();

  const corpo: [number, number, number, number][] = [
    [16, 16, 6, 6], [22, 16, 6, 6], [36, 16, 6, 6], [42, 16, 6, 6],
    [10, 22, 6, 6], [22, 22, 6, 6], [28, 22, 8, 6], [36, 22, 6, 6], [42, 22, 6, 6], [48, 22, 6, 6],
    [10, 28, 44, 6], [16, 34, 32, 6], [22, 40, 20, 6], [26, 46, 12, 6], [29, 52, 6, 6],
  ];
  const check: [number, number, number, number][] = [
    [20, 31, 4, 5], [24, 34, 4, 5], [28, 30, 4, 5], [32, 26, 4, 5], [36, 22, 4, 5],
  ];
</script>

<svg
  width={tamanho}
  height={tamanho}
  viewBox="10 16 44 42"
  shape-rendering="crispEdges"
  aria-hidden="true"
  focusable="false"
>
  {#each corpo as [x, y, w, h], i (i)}<rect {x} {y} width={w} height={h} fill="#ff3b4f" />{/each}
  <rect x="16" y="22" width="6" height="6" fill="#ffd6db" />
  {#each check as [x, y, w, h], i (i)}<rect {x} {y} width={w} height={h} fill="#0d1117" />{/each}
</svg>
