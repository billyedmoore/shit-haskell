class C a where
  f :: a -> Bool

instance C Bool where
  f :: Bool -> Bool

main = pure ()
