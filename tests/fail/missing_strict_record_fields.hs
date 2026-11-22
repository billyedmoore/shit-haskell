data Record = Record { count :: !Int, ready :: !Bool, note :: String }

main = print (note (Record { note = "ok" }))
