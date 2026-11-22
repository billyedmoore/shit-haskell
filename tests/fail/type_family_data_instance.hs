{-# LANGUAGE TypeFamilies #-}

type family Result a
data instance Result Int = Item

main = pure ()
