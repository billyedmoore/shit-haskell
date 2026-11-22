{-# LANGUAGE ImpredicativeTypes #-}

class C a
instance C (forall a. a -> a)

main = pure ()
