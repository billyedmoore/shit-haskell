{-# LANGUAGE NoFlexibleInstances #-}

class C a
instance C (Maybe Int)

main = pure ()
