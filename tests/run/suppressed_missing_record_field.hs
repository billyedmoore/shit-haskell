{-# OPTIONS_GHC -Werror -Wno-missing-fields #-}

data Record = Record { count :: !Int, note :: String }

main = print (count (Record { count = 7 }))
