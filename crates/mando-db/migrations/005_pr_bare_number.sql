-- Strip '#' prefix from tasks.pr to store bare numbers.
UPDATE tasks SET pr = SUBSTR(pr, 2) WHERE pr LIKE '#%';
