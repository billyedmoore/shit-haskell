{-# LANGUAGE NoTypeSynonymInstances #-}

type Alias = Bool
class C a
instance C Alias

main = pure ()
