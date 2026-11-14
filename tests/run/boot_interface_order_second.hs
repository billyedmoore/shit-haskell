-- The same modules loaded through the boot interface first. With one difftest worker,
-- this case runs before boot_interface_order_first and populates their shared cache.
module Main where
import CacheBootB
import CacheBootA

main :: IO ()
main = do
  print (makeFromB 7)
  print (scoreFromB (T 7), score (4 :: Int), score True)
