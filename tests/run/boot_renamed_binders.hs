import {-# SOURCE #-} BootRenamedBinders (keep)

main :: IO ()
main = print (keep (3 :: Int) False)
