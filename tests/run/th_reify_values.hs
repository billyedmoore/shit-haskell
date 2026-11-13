{-# LANGUAGE TemplateHaskell #-}
-- reify of the module's own functions (inferred and declared types), location, runIO and
-- recover, as QuickCheck's quickCheckAll uses them.
module Main where

import Props

prop_plain = 1 + 1 == (2 :: Int)

prop_reverse :: [Int] -> Bool
prop_reverse xs = reverse (reverse xs) == xs

prop_poly :: Eq a => a -> Bool
prop_poly x = x == x

prop_inferred n = n > (3 :: Integer)

{-
prop_absent is mentioned here but not defined: reify fails and recover skips it.
-}

helper :: Int
helper = 3

return []

main :: IO ()
main = do
  print $(here)
  mapM_ print $(allProps)
  print helper
