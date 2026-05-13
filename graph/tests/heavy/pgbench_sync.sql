BEGIN;
WITH ids AS (
    SELECT nextval('public.graph_pgbench_node_seq') AS id,
           nextval('public.graph_pgbench_node_seq') AS next_id
),
inserted AS (
    INSERT INTO public.graph_pgbench_nodes (id, name, score)
    SELECT id::text, 'name-' || id::text, (id % 1000)::int
    FROM ids
    RETURNING id
),
updated AS (
    UPDATE public.graph_pgbench_nodes n
    SET id = ids.next_id::text,
        name = 'renamed-' || ids.next_id::text,
        score = (ids.next_id % 1000)::int
    FROM ids
    WHERE n.id = ids.id::text
    RETURNING n.id
),
deleted AS (
    DELETE FROM public.graph_pgbench_nodes n
    USING ids
    WHERE n.id = ids.next_id::text
    RETURNING n.id
)
SELECT count(*) FROM inserted, updated, deleted;
COMMIT;
