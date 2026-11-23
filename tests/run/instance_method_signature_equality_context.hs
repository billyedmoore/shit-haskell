{-# LANGUAGE InstanceSigs, ConstrainedClassMethods, TypeFamilies #-}
class C a where
  method :: (b ~ Bool) => a -> b -> Bool
instance C Int where
  method :: Int -> Bool -> Bool
  method _ x = x

main :: IO ()
main = print (method (3 :: Int) True)
