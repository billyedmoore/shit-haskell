{-# LANGUAGE TypeFamilies #-}

type family Result a
type instance Result = Bool

main = pure ()
