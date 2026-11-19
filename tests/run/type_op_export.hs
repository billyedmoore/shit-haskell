-- An explicitly namespaced type operator exported with its constructors (sop-core's
-- `type (-.->)(..)`).
{-# LANGUAGE TypeOperators #-}
import Fn

main :: IO ()
main = do
  print (apFn (Fn (fmap (+ 1))) (Just (41 :: Int)))
  print (unFn (Fn reverse) "abc")
