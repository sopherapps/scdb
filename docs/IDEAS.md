# IDEAS

## Quickening Mutation

Since adding search has slowed down all mutative operations on the db, we need ways of reducing the speed drop without
compromising usability.

The folllowing are the options we have so far:

- Separate the processes doing the search index population and the db population, and run them concurrently.
    - Use a shared buffer to pass information between the two.
        - This might actually be faster at runtime but more complicated to pull off.
        - This can lose consistency permanently if a crash occurs or the app is stopped by user before indexing is
          complete.
    - Communicate using channels.
        - This will most likely be slower, and might be as complicated or more complicated than sharing a buffer (at
          least in rust)
        - This can lose consistency permanently if a crash occurs or the app is stopped by user before indexing is
          complete.
    - Use a Write-Ahead-Log(WAL) for Search Index Population.
        - This would allow the mutations to quickly append their log to the WAL log file and return to the caller.
        - The index population can be run as another process, reading directly from the WAL log file and updating the
          search index file.
        - This might require:
            - a proper format of log entries e.g. SIZE, TYPE, KEY, VALUE, IS_PROCESSED etc.
            - a predetermined file size beyond which a new WAL log file is created by the writer process i.e. the db
            - a way of flagging that a given entry has been processed e.g. using IS_PROCESSED.
            - a way of tracking the cursors and the path to the current WAL file being read, and that being written to.
        - This has a few gotchas:
            - the search could be inconsistent for a short time, unless a few tricks are applied
                - Tricks can include:
                    - Do the normal search, return the results, then do a hybrid search in the WAL itself, starting from
                      the cursor where the search was before the operation was started.
                    - Have a copy of the unprocessed WAL in memory but in the same data structure as the search index
                      i.e. an inverted index. After search on file is done, search in memory is done and the data
                      merged. The WAL copy in memory will have to be updated in realtime also when adding and removing
                      WAL logs.
- Optimize the indexing process itself.
    - Change the structure in which the indexing is done, to favour quick appends to the file.
        - E.g. We could have a structure that allows duplicate entries, with the more recent entries treated as more
          right