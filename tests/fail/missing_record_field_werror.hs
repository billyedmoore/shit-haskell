data Record = Record { count :: !Int, note :: String }

main = print (count (Record { count = 7 }))
