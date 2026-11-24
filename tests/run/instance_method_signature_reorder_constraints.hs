{-# LANGUAGE InstanceSigs, FlexibleContexts, ConstrainedClassMethods #-}
class C a where method :: (Show b, Eq b) => a -> b -> Bool
instance C Int where
  method :: (Eq b, Show b) => Int -> b -> Bool
  method _ x = x == x && not (null (show x))
main :: IO ()
main = print (method (3 :: Int) False)
