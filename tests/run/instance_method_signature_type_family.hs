{-# LANGUAGE InstanceSigs, TypeFamilies #-}
type family Payload a where
  Payload Bool = Char
class C a where
  method :: a -> Payload a
instance C Bool where
  method :: Bool -> Char
  method _ = 'x'

main :: IO ()
main = print (method True)
