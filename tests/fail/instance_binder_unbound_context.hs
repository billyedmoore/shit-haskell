{-# LANGUAGE UndecidableInstances #-}

class C a
instance forall a. Eq b => C [a]

main :: IO ()
main = pure ()
