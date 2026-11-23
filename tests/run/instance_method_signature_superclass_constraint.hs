{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: Ord b => a -> b -> Bool
instance C Int where
  method :: Eq b => Int -> b -> Bool
  method _ x = x == x
main :: IO ()
main = print (method (3 :: Int) False)
