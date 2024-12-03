# osm-replication-rust

osm-replication-rust is a tool to download OpenStreetMap diffs from planet, and
split them by polygons. The generated diffs can then be used to update a
smaller OpenStreetMap database.

Several libraries are included:
  - osmbin: optimised database only containing nodes coordinates, ways with
    their nodes, and full relations.
  - osmpbf: pbf reader
  - osmxml: xml reader/writer
  - osmxml/bbox: modify a diff by annotating ways and relation with a
    bounding-box of the impacted area, looking at previous and new coordinates.
  - osmxml/filter: keep only elements in a diff that are inside a given
    polygon, mark as "delete" elements in a small buffer, and remove elements
    that are outside this buffer.
  - diffs: recursively generate diffs from a given polygon directory.
  - update: download diff from planet, generate bbox and filtered diffs, and
    update local osmbin database.
