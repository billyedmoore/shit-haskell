{-# LANGUAGE TypeFamilies #-}

type family Result a

class D a where
  type Result a = Bool

main = pure ()
