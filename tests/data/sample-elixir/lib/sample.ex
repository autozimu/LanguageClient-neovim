defmodule Sample do
  @moduledoc """
  Documentation for Sample.
  """

  @doc """
  Hello world.

  ## Examples

      iex> Sample.hello()
      :world

  """
  def hello do
    :world
  end
end

defmodule Testing.Test do
  def aaa(<<"coolish", _rest::binary>>) do
    nil
  end

  def bbb() do
    nil
  end

  def ccc(%{id: id}) do

  end
end
