use scdb::Store;
use std::thread;
use std::time::Duration;

/// Prints data from store to the screen in a pretty way
macro_rules! pprint_data {
    ($title:expr, $data:expr) => {
        println!("\n");
        println!("{}", $title);
        println!("===============");

        for (k, got) in $data {
            let got_str = match got {
                None => "None",
                Some(v) => std::str::from_utf8(v).expect("bytes to str"),
            };
            println!("For key: '{}', str: '{}', raw: '{:?}',", k, got_str, got);
        }
    };
}

fn main() {
    // Creat the store. You can configure its `max_keys`, `redundant_blocks` etc. The defaults are usable though.
    // One very important config is `max_keys`. With it, you can limit the store size to a number of keys.
    // By default, the limit is 1 million keys
    let mut store =
        Store::new("db", Some(1000), Some(1), Some(10), Some(1800)).expect("create store");
    let records = [
        ("hey", "English"),
        ("hi", "English"),
        ("salut", "French"),
        ("bonjour", "French"),
        ("hola", "Spanish"),
        ("oi", "Portuguese"),
        ("mulimuta", "Runyoro"),
    ];
    let updates = [
        ("hey", "Jane"),
        ("hi", "John"),
        ("hola", "Santos"),
        ("oi", "Ronaldo"),
        ("mulimuta", "Aliguma"),
    ];
    let keys: Vec<&str> = records.iter().map(|(k, _)| *k).collect();

    // Setting the values
    println!("Let's insert data\n{:?}]...", &records);
    for (k, v) in &records {
        let _ = store.set(k.as_bytes(), v.as_bytes(), None);
    }

    // Getting the values (this is similar to what is in `get_all(&mut store, &keys)` function
    let data: Vec<(&str, Option<Vec<u8>>)> = keys
        .iter()
        .map(|k| (*k, store.get(k.as_bytes()).expect(&format!("get {}", k))))
        .collect();
    pprint_data!("After inserting data", &data);

    // Setting the values with time-to-live
    println!(
        "\n\nLet's insert data with 1 second time-to-live (ttl) for keys {:?}]...",
        &keys[3..]
    );
    for (k, v) in &records[3..] {
        let _ = store.set(k.as_bytes(), v.as_bytes(), Some(1));
    }

    println!("We will wait for 1 second to elapse...");
    thread::sleep(Duration::from_secs(2));

    let data = get_all(&mut store, &keys);
    pprint_data!("After inserting keys with ttl", &data);

    // Updating the values
    println!("\n\nLet's update with data {:?}]...", &updates);
    for (k, v) in &updates {
        let _ = store.set(k.as_bytes(), v.as_bytes(), None);
    }

    let data = get_all(&mut store, &keys);
    pprint_data!("After updating keys", &data);

    // Deleting some values
    let keys_to_delete = ["oi", "hi"];
    println!("\n\nLet's delete keys{:?}]...", &keys_to_delete);
    for k in keys_to_delete {
        store
            .delete(k.as_bytes())
            .expect(&format!("delete key {}", k));
    }

    let data = get_all(&mut store, &keys);
    pprint_data!("After deleting keys", &data);

    // Deleting all values
    println!("\n\nClear all data...");
    store.clear().expect("clear store");

    let data = get_all(&mut store, &keys);
    pprint_data!("After clearing", &data);
}

/// Gets all from store for the given keys
fn get_all<'a>(store: &mut Store, keys: &Vec<&'a str>) -> Vec<(&'a str, Option<Vec<u8>>)> {
    keys.iter()
        .map(|k| (*k, store.get(k.as_bytes()).expect(&format!("get {}", k))))
        .collect()
}
