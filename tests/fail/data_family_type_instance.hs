{-# LANGUAGE TypeFamilies #-}

data family Result a
type instance Result Int = Bool

main = pure ()
